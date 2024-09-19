use rocket::{get, http::Status, post, response::Redirect, State};
use rocket_dyn_templates::Template;

use crate::{
    auth::{
        csrf::{CsrfToken, VerifyCsrfToken},
        users::{Admin, User},
    },
    contests::{team::Team, Contest},
    context_with_base_authed,
    db::DbConnection,
    error::prelude::*,
    leaderboard::LeaderboardManagerHandle,
    messages::Message,
};

#[get("/contests/<contest_id>/admin/participants")]
pub async fn participants(
    mut db: DbConnection,
    user: &User,
    admin: Option<&Admin>,
    contest_id: i64,
) -> ResultResponse<Template> {
    let contest = Contest::get_or_404_assert_can_edit(&mut db, contest_id, user, admin).await?;
    let teams = contest.list_teams(&mut db).await?;
    let ctx = context_with_base_authed!(user, contest, teams);
    Ok(Template::render("contests/admin/participants", ctx))
}

// TODO: Implement for teams & specific members
// #[get("/contests/<contest_id>/admin/participants/<p_id>/kick")]
// pub async fn kick_participant_get(
//     contest_id: i64,
//     p_id: i64,
//     mut db: DbConnection,
//     user: &User,
//     _token: &CsrfToken,
//     admin: Option<&Admin>,
// ) -> ResultResponse<Template> {
//     let contest = Contest::get_or_404_assert_can_edit(&mut db, contest_id, user, admin).await?;
//     let target_participant = Team::by_id(&mut db, p_id)
//         .await?
//         .ok_or(Status::NotFound)?;
//     let ctx = context_with_base_authed!(user, contest, target_participant, target_user);
//     Ok(Template::render("contests/admin/kick", ctx))
// }

// #[post("/contests/<contest_id>/admin/participants/<p_id>/kick")]
// pub async fn kick_participant_post(
//     contest_id: i64,
//     p_id: i64,
//     mut db: DbConnection,
//     leaderboards: &State<LeaderboardManagerHandle>,
//     user: &User,
//     _token: &VerifyCsrfToken,
//     admin: Option<&Admin>,
// ) -> ResultResponse<Redirect> {
//     Contest::get_or_404_assert_can_edit(&mut db, contest_id, user, admin).await?;
//     let target_participant = Participant::by_id(&mut db, p_id)
//         .await?
//         .ok_or(Status::NotFound)?;
//     target_participant.delete(&mut db).await.map_err(|e| {
//         log::error!("Failed to delete participant: {:?}", e);
//         Status::InternalServerError
//     })?;
//     let mut leaderboard_manager = leaderboards.lock().await;
//     leaderboard_manager
//         .delete_participant_for_contest(p_id, contest_id)
//         .await;
//     Ok(Message::success("Participant Kicked")
//         .to(&format!("/contests/{}/admin/participants", contest_id)))
// }
