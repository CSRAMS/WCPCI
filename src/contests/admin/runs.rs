use chrono::TimeZone;
use log::error;
use rocket::{get, http::Status, post, response::Redirect, State};
use rocket_dyn_templates::Template;

use crate::{
    auth::users::{Admin, User},
    contests::{team::Team, Contest},
    context_with_base_authed,
    db::DbConnection,
    error::prelude::*,
    messages::Message,
    problems::{JudgeRun, Problem, ProblemCompletion},
    run::ManagerHandle,
    times::{format_datetime_human_readable, ClientTimeZone},
};

#[derive(Serialize)]
struct TempProblem {
    id: i64,
    slug: String,
    contest_id: i64,
}

#[derive(Serialize)]
pub struct RunsAdminRow {
    user: User,
    problem: TempProblem,
}

#[get("/contests/<contest_id>/admin/runs")]
pub async fn runs(
    mut db: DbConnection,
    user: &User,
    contest_id: i64,
    admin: Option<&Admin>,
    manager_handle: &State<ManagerHandle>,
) -> ResultResponse<Template> {
    let contest = Contest::get_or_404_assert_can_edit(&mut db, contest_id, user, admin).await?;
    let manager = manager_handle.lock().await;
    let jobs = manager.all_active_jobs().await;
    drop(manager);
    let mut rows = Vec::with_capacity(jobs.len());
    for (job_user_id, problem_id) in jobs {
        let job_user = User::get(&mut db, job_user_id)
            .await
            .context("Failed while getting runs")?
            .ok_or_else(|| {
                error!("Couldn't find user with id {}", job_user_id);
                Status::InternalServerError
            })?;
        let problem = sqlx::query_as!(
            TempProblem,
            "SELECT id, slug, contest_id FROM problem WHERE id = ? AND contest_id = ?",
            problem_id,
            contest_id
        )
        .fetch_optional(&mut **db)
        .await
        .map_err(|e| {
            error!("Couldn't find problem with id {}: {:?}", problem_id, e);
            Status::InternalServerError
        })?;
        if let Some(problem) = problem {
            rows.push(RunsAdminRow {
                user: job_user,
                problem,
            });
        }
    }

    let problems = Problem::list(&mut db, contest_id).await?;

    let ctx = context_with_base_authed!(user, rows, contest, problems);
    Ok(Template::render("contests/admin/runs", ctx))
}

#[get("/contests/<contest_id>/admin/runs/<user_id>/<problem_id>/cancel")]
pub async fn cancel(
    mut db: DbConnection,
    user: &User,
    contest_id: i64,
    user_id: i64,
    problem_id: i64,
    admin: Option<&Admin>,
    manager_handle: &State<ManagerHandle>,
) -> ResultResponse<Template> {
    let contest = Contest::get_or_404_assert_can_edit(&mut db, contest_id, user, admin).await?;
    let problem = Problem::by_id(&mut db, contest_id, problem_id)
        .await?
        .ok_or(Status::NotFound)?;

    let manager = manager_handle.lock().await;
    manager
        .get_handle(user_id, problem_id)
        .await
        .ok_or(Status::NotFound)?;
    drop(manager);
    let target_user = User::get(&mut db, user_id)
        .await
        .context("While getting run")?
        .ok_or(Status::NotFound)?;
    Ok(Template::render(
        "contests/admin/runs_cancel",
        context_with_base_authed!(user, target_user, contest, problem),
    ))
}

#[post("/contests/<contest_id>/admin/runs/<user_id>/<problem_id>/cancel")]
pub async fn cancel_post(
    mut db: DbConnection,
    user: &User,
    contest_id: i64,
    user_id: i64,
    problem_id: i64,
    admin: Option<&Admin>,
    manager_handle: &State<ManagerHandle>,
) -> ResultResponse<Redirect> {
    Contest::get_or_404_assert_can_edit(&mut db, contest_id, user, admin).await?;
    Problem::by_id(&mut db, contest_id, problem_id)
        .await?
        .ok_or(Status::NotFound)?;
    let mut manager = manager_handle.lock().await;
    manager
        .get_handle(user_id, problem_id)
        .await
        .ok_or(Status::NotFound)?;
    manager.shutdown_job(user_id).await;
    Ok(Message::success("Run Cancelled").to(&format!("/contests/{}/admin/runs", contest_id)))
}

// TODO: Implement for teams & specific members
// #[derive(Serialize)]
// struct CompletionsRow {
//     team: Team,
//     pub completion: ProblemCompletion,
// }

// #[get("/contests/<contest_id>/admin/runs/problems/<problem_slug>")]
// pub async fn problem(
//     mut db: DbConnection,
//     user: &User,
//     contest_id: i64,
//     tz: ClientTimeZone,
//     problem_slug: &str,
//     admin: Option<&Admin>,
// ) -> ResultResponse<Template> {
//     let contest = Contest::get_or_404_assert_can_edit(&mut db, contest_id, user, admin).await?;
//     let problem = Problem::get_or_404(&mut db, contest_id, problem_slug).await?;
//     let mut rows = Vec::new();
//     let teams = contest.list_teams(&mut db).await?;
//     for t in teams {
//         let completion =
//             ProblemCompletion::get_for_problem_and_team(&mut db, problem.id, t.id)
//                 .await?
//                 .unwrap_or(ProblemCompletion {
//                     team_id: t.id,
//                     problem_id: problem.id,
//                     completed_at: None,
//                     number_wrong: 0,
//                 });
//         rows.push(CompletionsRow {
//             team: t,
//             completion,
//         });
//     }

//     let tz = tz.timezone();
//     let formatted_times = rows
//         .iter()
//         .map(|r| {
//             r.completion
//                 .completed_at
//                 .map(|c| format_datetime_human_readable(tz.from_utc_datetime(&c)))
//                 .unwrap_or_else(|| "Not Completed".to_string())
//         })
//         .collect::<Vec<_>>();

//     let ctx = context_with_base_authed!(user, rows, formatted_times, contest, problem);
//     Ok(Template::render("contests/admin/runs_problem", ctx))
// }

// #[get("/contests/<contest_id>/admin/runs/problems/<problem_slug>/view/<team_id>")]
// pub async fn view_user_run(
//     mut db: DbConnection,
//     user: &User,
//     contest_id: i64,
//     team_id: i64,
//     problem_slug: &str,
//     admin: Option<&Admin>,
// ) -> ResultResponse<Template> {
//     let contest =
//         Contest::get_or_404_assert_can_edit(&mut db, contest_id, user, admin).await?;
//     let problem = Problem::get_or_404(&mut db, contest_id, problem_slug).await?;
//     let target_team = Team::by_id(&mut db, team_id)
//         .await?
//         .ok_or(Status::NotFound)?;
//     let most_recent = JudgeRun::get_latest(&mut db, target_team.user_id, problem.id).await?;
//     let success_recent =
//         JudgeRun::get_latest_success(&mut db, target_team.user_id, problem.id).await?;
//     Ok(Template::render(
//         "contests/admin/runs_view",
//         context_with_base_authed!(
//             user,
//             target_user,
//             contest,
//             problem,
//             most_recent,
//             success_recent
//         ),
//     ))
// }
