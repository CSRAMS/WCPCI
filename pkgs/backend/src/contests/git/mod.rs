use std::{collections::HashMap, sync::Arc};

use chrono::NaiveDateTime;
use rand::distr::Alphanumeric;
use rand::Rng;
use repo::FakeRepo;
use rocket::{fairing::AdHoc, http::Status, State};
use rocket_dyn_templates::Template;
use tokio::sync::Mutex;

use crate::{
    auth::users::{Admin, User},
    branding::BrandingConfig,
    contests::git::{commit::Commit, refs::Ref},
    context_with_base_authed,
    db::DbConnection,
    error::prelude::*,
    problems::{JudgeRun, Problem},
    run::CodeInfo,
};

use self::{
    object::{Object, ObjectType},
    tree::Tree,
};

use super::Contest;

mod commit;
mod object;
mod refs;
mod repo;
mod store;
mod tree;

type RepoMap = HashMap<(i64, i64), (String, FakeRepo, NaiveDateTime)>;
type RepoMapHandle = Arc<Mutex<RepoMap>>;
type RepoMapGuard<'a> = &'a State<RepoMapHandle>;

fn run_to_object(run: &JudgeRun) -> Result<Object> {
    Object::new(run.program.as_bytes().to_vec(), ObjectType::Blob)
}

fn gen_code() -> String {
    rand::rng()
        .sample_iter(&Alphanumeric)
        .take(16)
        .map(char::from)
        .collect()
}

const CACHE_TIME_MINUTES: usize = 5;

#[get("/contests/<contest_id>/export")]
pub async fn export_solutions(
    user: &User,
    contest_id: i64,
    mut db: DbConnection,
    admin: Option<&Admin>,
    info: &State<CodeInfo>,
    branding: &State<BrandingConfig>,
    repos_handle: RepoMapGuard<'_>,
) -> ResultResponse<Template> {
    let (contest, _participant, can_edit) =
        Contest::get_or_404_assert_started(&mut db, contest_id, Some(user), admin).await?;

    let now = chrono::Utc::now().naive_utc();

    let repos = repos_handle.lock().await;
    if let Some((code, _, generated)) = repos.get(&(contest_id, user.id)) {
        if now - *generated < chrono::Duration::minutes(CACHE_TIME_MINUTES as i64) {
            let ctx = context_with_base_authed!(user, code, generated, contest, can_edit);
            return Ok(Template::render("contests/export", ctx));
        }
    }
    drop(repos);

    let problems = Problem::list(&mut db, contest_id).await?;
    let mut runs = Vec::with_capacity(problems.len());
    for problem in problems.iter() {
        let latest_successful_run =
            JudgeRun::get_latest_success(&mut db, user.id, problem.id).await?;
        let latest_run = JudgeRun::get_latest(&mut db, user.id, problem.id).await?;
        runs.push((
            latest_successful_run.map(|r| {
                let obj = run_to_object(&r)
                    .context("Failed to serialize run")
                    .unwrap();
                (r, obj)
            }),
            latest_run.map(|r| {
                let obj = run_to_object(&r)
                    .context("Failed to serialize run")
                    .unwrap();
                (r, obj)
            }),
        ));
    }

    let mut repo = FakeRepo::new();

    // Add all the runs to the repo
    for (sr, mr) in runs.iter() {
        if let Some((_, obj)) = sr {
            repo.add_object(obj.clone());
        }
        if let Some((_, obj)) = mr {
            repo.add_object(obj.clone());
        }
    }

    const BLOB_MODE: &str = "100644";
    const DIR_MODE: &str = "040000";

    let problem_description_objs = problems
        .iter()
        .map(|p| {
            let md = format!("# {}\n\n{}\n", p.name, p.description.trim());
            Object::new(md.as_bytes().to_vec(), ObjectType::Blob)
                .context("Failed to serialize problem description")
        })
        .collect::<Result<Vec<_>>>()?;

    // Add all problem descriptions to the repo
    for obj in problem_description_objs.iter() {
        repo.add_object(obj.clone());
    }

    // Now make trees to represent folders for each problem
    let problem_trees = runs
        .iter()
        .zip(problem_description_objs.iter())
        .map(|(runs, problems)| {
            let mut tree = tree::Tree::new();
            tree.add_entry(
                BLOB_MODE.to_string(),
                problems.get_hash(),
                "description.md".to_string(),
            );
            if let Some((run, obj)) = &runs.0 {
                let ext = info
                    .run_config
                    .languages
                    .get(&run.language)
                    .and_then(|l| l.runner.file_name.split('.').next_back())
                    .unwrap_or("txt");
                tree.add_entry(
                    BLOB_MODE.to_string(),
                    obj.get_hash(),
                    format!("most-recent-success.{ext}"),
                );
            }
            if let Some((run, obj)) = &runs.1 {
                let ext = info
                    .run_config
                    .languages
                    .get(&run.language)
                    .and_then(|l| l.runner.file_name.split('.').next_back())
                    .unwrap_or("txt");
                tree.add_entry(
                    BLOB_MODE.to_string(),
                    obj.get_hash(),
                    format!("most-recent.{ext}"),
                );
            }
            let obj = tree.to_object().unwrap();
            (tree, obj)
        })
        .collect::<Vec<_>>();

    // Add all problem trees to the repo
    for tree in problem_trees.iter() {
        repo.add_object(tree.1.clone());
    }

    let problems_txt = problems
        .iter()
        .map(|p| format!("- [{}]({}/)", p.name, p.slug))
        .collect::<Vec<_>>()
        .join("\n");

    // TODO: Use branding name
    let readme = format!(
        "# Solutions for {name}\n\nThis repo contains the solutions for {name} by {display_name}\n\n## Problems\n\n{problems_txt}\n\nGenerated by {site_name} {version}\n",
        name = contest.name,
        display_name = user.display_name(),
        version = env!("CARGO_PKG_VERSION"),
        site_name = branding.name,
    );

    let readme_obj = Object::new(readme.as_bytes().to_vec(), ObjectType::Blob)
        .context("Failed to serialize README")?;

    // Add README to the repo
    repo.add_object(readme_obj.clone());

    let mut root_tree = Tree::new();

    for (tree, problem) in problem_trees.iter().zip(problems.iter()) {
        root_tree.add_entry(
            DIR_MODE.to_string(),
            tree.1.get_hash(),
            problem.slug.clone(),
        );
    }

    root_tree.add_entry(
        BLOB_MODE.to_string(),
        readme_obj.get_hash(),
        "README.md".to_string(),
    );

    // Add root tree to the repo
    repo.add_object(root_tree.to_object()?);

    let root_hash = root_tree.to_object()?.get_hash_str();

    let now_epoch = now.and_utc().timestamp();
    let author = format!("Solution Exporter <solution-export@example.com> {now_epoch} +0000");
    let commit = Commit::new(
        root_hash.clone(),
        String::new(),
        author.clone(),
        author,
        String::new(),
        "Initial Commit".to_string(),
    );

    let commit_obj = commit.to_object()?;
    let commit_hash = commit_obj.get_hash_str();

    // Add commit to the repo
    repo.add_object(commit_obj);

    repo.add_head("main", Ref::Object(commit_hash.clone()));
    repo.add_tag("import", Ref::Object(commit_hash));

    let code = gen_code();
    let now = chrono::Utc::now().naive_utc();

    let mut repos = repos_handle.lock().await;

    repos.insert((contest_id, user.id), (code.clone(), repo, now));

    let ctx = context_with_base_authed!(user, code, contest, can_edit);
    Ok(Template::render("contests/export", ctx))
}

#[get("/contests/<contest_id>/export/<user_id>/<code>/solutions.git/info/refs")]
async fn git_info_refs(
    contest_id: i64,
    user_id: i64,
    code: &str,
    repos_handle: RepoMapGuard<'_>,
) -> ResultResponse<String> {
    let repos = repos_handle.lock().await;
    let (real_code, repo, generated) = repos.get(&(contest_id, user_id)).ok_or(Status::NotFound)?;
    let now = chrono::Utc::now().naive_utc();
    if now - *generated > chrono::Duration::minutes(CACHE_TIME_MINUTES as i64) || code != real_code
    {
        return Err(Status::NotFound.into());
    }

    Ok(repo.dump_refs())
}

#[get("/contests/<contest_id>/export/<user_id>/<code>/solutions.git/objects/<folder>/<rest>")]
async fn git_objects(
    contest_id: i64,
    user_id: i64,
    code: &str,
    folder: &str,
    rest: &str,
    repos_handle: RepoMapGuard<'_>,
) -> ResultResponse<Vec<u8>> {
    let repos = repos_handle.lock().await;
    let (real_code, repo, generated) = repos.get(&(contest_id, user_id)).ok_or(Status::NotFound)?;
    let now = chrono::Utc::now().naive_utc();
    if now - *generated > chrono::Duration::minutes(CACHE_TIME_MINUTES as i64) || code != real_code
    {
        return Err(Status::NotFound.into());
    }

    let obj = repo.get_object(folder, rest).ok_or(Status::NotFound)?;

    Ok(obj.compressed_serialize()?)
}

#[get("/contests/<contest_id>/export/<user_id>/<code>/solutions.git/HEAD")]
async fn git_head(
    contest_id: i64,
    user_id: i64,
    code: &str,
    repos_handle: RepoMapGuard<'_>,
) -> ResultResponse<String> {
    let repos = repos_handle.lock().await;
    let (real_code, _repo, generated) =
        repos.get(&(contest_id, user_id)).ok_or(Status::NotFound)?;
    let now = chrono::Utc::now().naive_utc();
    if now - *generated > chrono::Duration::minutes(CACHE_TIME_MINUTES as i64) || code != real_code
    {
        return Err(Status::NotFound.into());
    }

    let main_ref = Ref::Forward("refs/heads/main".to_string());
    Ok(main_ref.to_string())
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("Git Export", |rocket| async {
        let repo_map = RepoMapHandle::new(Mutex::new(HashMap::new()));
        let handle_clone = repo_map.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(
                    60 * 2 * CACHE_TIME_MINUTES as u64,
                ))
                .await;
                let now = chrono::Utc::now().naive_utc();
                let mut repos = handle_clone.lock().await;
                repos.retain(|_, (_, _, generated)| {
                    now - *generated < chrono::Duration::minutes(CACHE_TIME_MINUTES as i64)
                });
            }
        });
        rocket.manage(repo_map).mount(
            "/",
            routes![export_solutions, git_info_refs, git_objects, git_head],
        )
    })
}
