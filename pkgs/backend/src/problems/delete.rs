use rocket::{get, post, State};
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
};

use super::Problem;

#[get("/<contest_id>/problems/<slug>/delete")]
pub async fn delete_problem_get(
    user: &User,
    admin: Option<&Admin>,
    contest_id: i64,
    mut db: DbConnection,
    slug: &str,
    _token: &CsrfToken,
) -> ResultResponse<Template> {
    let (contest, _) =
        Contest::get_or_404_assert_can_edit(&mut db, contest_id, user, admin).await?;
    let problem = Problem::get_or_404(&mut db, contest_id, slug).await?;
    Ok(Template::render(
        "problems/delete",
        context_with_base_authed!(user, contest, problem),
    ))
}

#[post("/<contest_id>/problems/<slug>/delete")]
pub async fn delete_problem_post(
    user: &User,
    admin: Option<&Admin>,
    contest_id: i64,
    slug: &str,
    _token: &VerifyCsrfToken,
    leaderboard_handle: &State<LeaderboardManagerHandle>,
    mut db: DbConnection,
) -> FormResponse {
    let (contest, _) =
        Contest::get_or_404_assert_can_edit(&mut db, contest_id, user, admin).await?;
    let problem = Problem::get_or_404(&mut db, contest_id, slug).await?;
    problem.delete(&mut db).await?;
    let mut leaderboard_handle = leaderboard_handle.lock().await;
    leaderboard_handle
        .refresh_leaderboard(&mut db, &contest)
        .await?;

    Ok(Message::success("Problem Deleted").to(&format!("/contests/{}/problems", contest_id)))
}
