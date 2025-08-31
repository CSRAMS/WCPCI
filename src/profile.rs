use chrono::TimeZone;
use rocket::{fairing::AdHoc, get, routes, State};
use rocket_dyn_templates::Template;

use crate::{
    auth::users::User,
    contests::{Contest, Participant},
    context_with_base,
    db::DbConnection,
    leaderboard::LeaderboardManagerHandle,
    problems::Problem,
    times::ClientTimeZone,
    ResultResponse,
};

#[derive(Serialize)]
struct ProfileContestEntry {
    id: i64,
    name: String,
    solved: usize,
    total: usize,
    role: String,
    rank: usize,
}

#[get("/<user_id>")]
async fn profile(
    mut db: DbConnection,
    leaderboard_manager: &State<LeaderboardManagerHandle>,
    user_id: i64,
    tz: ClientTimeZone,
    user: Option<&User>,
) -> ResultResponse<Template> {
    let profile = User::get_or_404(&mut db, user_id).await?;
    let joined = tz
        .timezone()
        .from_utc_datetime(&profile.created_at)
        .format("%B %-d, %Y")
        .to_string();
    let is_me = user.is_some_and(|u| u.id == user_id);

    let contests = Contest::list_user_in(&mut db, user_id).await?;

    let mut contest_entries = Vec::<ProfileContestEntry>::with_capacity(contests.len());

    for contest in contests {
        let mut leaderboards = leaderboard_manager.lock().await;
        let leaderboard = leaderboards.get_leaderboard(&mut db, &contest).await?;
        drop(leaderboards);
        let leaderboard = leaderboard.lock().await;
        let stats = leaderboard.stats_of(user_id);
        let problems_total = Problem::list(&mut db, contest.id).await?.len();
        if let Some((solved, rank)) = stats {
            let role = Participant::get(&mut db, contest.id, user_id)
                .await?
                .map(|p| if p.is_judge { "Judge" } else { "Participant" })
                .unwrap_or("Participant");
            contest_entries.push(ProfileContestEntry {
                id: contest.id,
                name: contest.name,
                solved,
                total: problems_total,
                role: role.to_string(),
                rank,
            });
        }
    }

    let ctx = context_with_base!(user, contests: contest_entries, is_me, joined, profile);
    Ok(Template::render("profile", ctx))
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("User Profiles", |rocket| async {
        rocket.mount("/profile", routes![profile])
    })
}
