use log::error;
use rocket::{http::Status, post, response::Redirect, State};

use crate::{
    auth::users::{Admin, User},
    db::DbConnection,
    leaderboard::LeaderboardManagerHandle,
    messages::Message,
    FormResponse,
};

use super::{Contest, Participant};

#[post("/<contest_id>/join", rank = 10)]
pub async fn join_contest(
    mut db: DbConnection,
    contest_id: i64,
    leaderboard_handle: &State<LeaderboardManagerHandle>,
    user: &User,
    admin: Option<&Admin>,
) -> FormResponse {
    let contest = Contest::get_or_404(&mut db, contest_id).await?;
    if admin.is_some()
        || Participant::get(&mut db, contest_id, user.id)
            .await?
            .is_some()
    {
        Ok(Redirect::to(format!("/contests/{}/", contest_id)))
    } else if contest.can_register() {
        if let Some(max_participants) = &contest.max_participants {
            let participants = Participant::list_not_judge(&mut db, contest_id).await?;
            if participants.len() >= *max_participants as usize {
                return Err(Status::Forbidden.into());
            }
        }
        let participant = Participant::temp(user.id, contest_id, false);
        if let Err(why) = participant.insert(&mut db).await {
            error!("Error inserting participant: {:?}", why);
            Err(Status::InternalServerError.into())
        } else {
            let mut leaderboard_manager = leaderboard_handle.lock().await;
            leaderboard_manager
                .refresh_leaderboard(&mut db, &contest)
                .await?;

            Ok(Message::success(&format!("Welcome to {}!", contest.name))
                .to(&format!("/contests/{}/", contest_id)))
        }
    } else {
        Err(Status::Forbidden.into())
    }
}
