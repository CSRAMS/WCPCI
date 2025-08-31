use rocket::{get, State};
use rocket_dyn_templates::Template;

use crate::{
    auth::users::{Admin, User},
    contests::{Contest, Participant},
    context_with_base,
    db::DbConnection,
    error::prelude::*,
    run::CodeInfo,
};

use super::{JudgeRun, Problem, ProblemCompletion, TestCase};

#[get("/<contest_id>/problems")]
pub async fn list_problems_get(
    user: Option<&User>,
    admin: Option<&Admin>,
    contest_id: i64,
    mut db: DbConnection,
) -> ResultResponse<Template> {
    let contest = Contest::get_or_404(&mut db, contest_id).await?;
    let participant = if let Some(user) = user {
        Participant::get(&mut db, contest_id, user.id).await?
    } else {
        None
    };
    let is_judge = participant.as_ref().is_some_and(|p| p.is_judge);
    let is_admin = admin.is_some();
    let can_see = is_admin || is_judge || contest.has_started();
    let problems = if can_see {
        Problem::list(&mut db, contest_id).await?
    } else {
        vec![]
    };
    Ok(Template::render(
        "problems",
        context_with_base!(user, problems, is_admin, participant, started: can_see, contest, can_edit: is_judge || is_admin),
    ))
}

#[get("/<contest_id>/problems/<slug>", rank = 10)]
pub async fn view_problem_get(
    user: Option<&User>,
    admin: Option<&Admin>,
    info: &State<CodeInfo>,
    mut db: DbConnection,
    contest_id: i64,
    slug: &str,
) -> ResultResponse<Template> {
    let (contest, participant, can_edit) =
        Contest::get_or_404_assert_started(&mut db, contest_id, user, admin).await?;
    let problem = Problem::get_or_404(&mut db, contest_id, slug).await?;

    let completion = if let Some(ref participant) = participant {
        ProblemCompletion::get_for_problem_and_participant(&mut db, problem.id, participant.p_id)
            .await?
    } else {
        None
    };

    let case_count = TestCase::count_for_problem(&mut db, problem.id)
        .await
        .unwrap_or(0);

    let last_run = if let Some(user) = user {
        JudgeRun::get_latest(&mut db, user.id, problem.id).await?
    } else {
        None
    };

    let most_recent_code = serde_json::to_string(
        &last_run
            .as_ref()
            .map(|lr| (lr.program.as_str(), lr.language.as_str())),
    )
    .context("Failed to serialize most recent code")?;

    let last_run = last_run
        .filter(|r| r.total_cases == case_count) // Don't show runs when test cases have changed
        .filter(|r| {
            r.error.is_some() || completion.map(|c| c.completed_at.is_some()).unwrap_or(true)
        }); // Don't show run if judge overrode completion

    let languages = info.run_config.get_languages_for_dropdown();
    let code_info = &info.languages_json;
    let default_language = user
        .map(|u| &u.default_language)
        .filter(|l| info.run_config.languages.contains_key(*l))
        .unwrap_or(&info.run_config.default_language);

    Ok(Template::render(
        "problems/view",
        context_with_base!(
            user,
            problem,
            last_run,
            case_count,
            most_recent_code,
            ended: contest.has_ended(),
            contest,
            code_info,
            languages,
            default_language,
            can_edit,
            participating: participant.is_some_and(|p| !p.is_judge),
        ),
    ))
}
