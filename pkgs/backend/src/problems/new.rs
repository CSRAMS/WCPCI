use rocket::{
    form::{Contextual, Error, Form},
    get, post, State,
};
use rocket_dyn_templates::Template;

use crate::{
    auth::{
        csrf::{CsrfToken, VerifyCsrfToken},
        users::{Admin, User},
    },
    contests::Contest,
    context_with_base_authed,
    db::DbConnection,
    error::prelude::*,
    leaderboard::LeaderboardManagerHandle,
    messages::Message,
    template::FormTemplateObject,
};

use super::{cases::TestCase, Problem, ProblemForm, ProblemFormTemplate};

#[get("/<contest_id>/problems/new", rank = 1)]
pub async fn new_problem_get(
    mut db: DbConnection,
    user: &User,
    admin: Option<&Admin>,
    contest_id: i64,
    _token: &CsrfToken,
) -> ResultResponse<Template> {
    let (contest, _) =
        Contest::get_or_404_assert_can_edit(&mut db, contest_id, user, admin).await?;
    let form_template = ProblemFormTemplate {
        problem: None,
        test_cases: vec![],
    };
    let form = FormTemplateObject::get(form_template);
    Ok(Template::render(
        "problems/new",
        context_with_base_authed!(user, contest, form),
    ))
}

#[post("/<contest_id>/problems/new", data = "<form>", rank = 5)]
pub async fn new_problem_post(
    user: &User,
    admin: Option<&Admin>,
    contest_id: i64,
    mut form: Form<Contextual<'_, ProblemForm<'_>>>,
    _token: &VerifyCsrfToken,
    leaderboard_handle: &State<LeaderboardManagerHandle>,
    mut db: DbConnection,
) -> FormResponse {
    let (contest, _) =
        Contest::get_or_404_assert_can_edit(&mut db, contest_id, user, admin).await?;

    if let Some(ref value) = form.value {
        let problem = Problem::temp(contest_id, value);
        if Problem::slug_exists(&mut db, &problem.slug, contest_id, None).await? {
            let err = Error::validation("Problem with this name already exists").with_name("name");
            form.context.push_error(err);
        } else if value.test_cases.is_empty() {
            let err =
                Error::validation("At least one test case is required").with_name("test_cases");
            form.context.push_error(err);
        } else {
            let problem = problem.insert(&mut db).await?;
            let test_cases = TestCase::from_vec(problem.id, &value.test_cases);
            TestCase::save_for_problem(&mut db, problem.id, test_cases).await?;
            let mut leaderboard_handle = leaderboard_handle.lock().await;
            leaderboard_handle
                .refresh_leaderboard(&mut db, &contest)
                .await?;
            return Ok(Message::success("Problem Created")
                .to(&format!("/contests/{contest_id}/problems/{}", problem.slug)));
        }
    }

    let form_template = ProblemFormTemplate {
        problem: None,
        test_cases: vec![],
    };
    let form = FormTemplateObject::from_rocket_context(form_template, &form.context);

    Err(Template::render(
        "problems/new",
        context_with_base_authed!(user, contest, form),
    )
    .into())
}
