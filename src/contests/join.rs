// TODO: Overhaul for teams 

use log::error;
use rocket::{http::Status, post, response::Redirect, State};

use crate::{
    auth::users::{Admin, User},
    db::DbConnection,
    leaderboard::LeaderboardManagerHandle,
    FormResponse,
};

use super::{Contest, Judge};

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
        || Judge::is_judge(user.id, contest_id, &mut db)
            .await?
    {
        Ok(Redirect::to(format!("/contests/{}/", contest_id)))
    } else if contest.can_register() {
        // TODO: Overhaul joining for teams

        // if let Some(max_teams) = &contest.max_teams {
        //     let teams = Team::list_not_judge(&mut db, contest_id).await?;
        //     if teams.len() >= *max_teams as usize {
        //         return Err(Status::Forbidden.into());
        //     }
        // }
        // let team = Team::temp(user.id, contest_id, false);
        // if let Err(why) = team.insert(&mut db).await {
        //     error!("Error inserting team: {:?}", why);
        //     Err(Status::InternalServerError.into())
        // } else {
        //     let mut leaderboard_manager = leaderboard_handle.lock().await;
        //     leaderboard_manager
        //         .refresh_leaderboard(&mut db, &contest)
        //         .await?;

        //     Ok(Message::success(&format!("Welcome to {}!", contest.name))
        //         .to(&format!("/contests/{}/", contest_id)))
        // }
        Err(Status::ImATeapot.into())
    } else {
        Err(Status::Forbidden.into())
    }
}
