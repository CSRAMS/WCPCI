use std::{collections::HashMap, sync::Arc};

use chrono::TimeZone;
use rocket::{fairing::AdHoc, get, routes, State};

mod manager;
mod scoring;
mod ws;

pub use manager::{LeaderboardManager, LeaderboardManagerHandle};
use rocket_dyn_templates::Template;
use tokio::sync::Mutex;

use crate::{
    auth::users::{Admin, User},
    contests::{Contest, Judge},
    context_with_base,
    db::DbConnection,
    error::prelude::*,
    times::{datetime_to_html_time, ClientTimeZone},
};

use self::ws::leaderboard_ws;

#[derive(Serialize)]
struct ProblemIdTemp {
    pub id: i64,
    pub slug: String,
    pub name: String,
}

#[get("/contests/<contest_id>/leaderboard")]
async fn leaderboard_get(
    mut db: DbConnection,
    leaderboard_manager: &State<LeaderboardManagerHandle>,
    contest_id: i64,
    tz: ClientTimeZone,
    user: Option<&User>,
    admin: Option<&Admin>,
) -> ResultResponse<Template> {
    let contest = Contest::get_or_404(&mut db, contest_id).await?;
    let mut leaderboard_manager = leaderboard_manager.lock().await;
    let leaderboard = leaderboard_manager
        .get_leaderboard(&mut db, &contest)
        .await?
        .clone();
    drop(leaderboard_manager);
    let mut leaderboard = leaderboard.lock().await;

    let problems = sqlx::query_as!(
        ProblemIdTemp,
        "SELECT id, slug, name from problem WHERE contest_id = ?",
        contest.id
    )
    .fetch_all(&mut **db)
    .await
    .context("Failed to fetch problems")?;

    let is_judge = if let Some(user) = user {
        Judge::is_judge(user.id, contest_id, &mut db).await?
    } else {
        false
    };

    let entries = leaderboard.full(&mut db).await?;
    let is_frozen = leaderboard.is_frozen();

    let start_local = tz.timezone().from_utc_datetime(&contest.start_time);
    let start_local_html = datetime_to_html_time(&start_local);
    let end_local = tz.timezone().from_utc_datetime(&contest.end_time);
    let end_local_html = datetime_to_html_time(&end_local);

    let first_map = leaderboard
        .first_map
        .iter()
        .map(|(k, v)| (k.to_string(), v))
        .collect::<HashMap<_, _>>();

    Ok(Template::render(
        "contests/leaderboard",
        context_with_base!(user, is_frozen, first_map, freeze_percent: contest.freeze_percent(), progress: contest.progress(), has_started: contest.has_started(), start_local_html, end_local_html, is_running: contest.is_running(), contest, entries, problems, is_admin: admin.is_some(), is_judge),
    ))
}

pub fn stage() -> AdHoc {
    let (tx, rx) = tokio::sync::watch::channel(false);

    AdHoc::on_ignite("Leaderboard App", |rocket| async {
        let shutdown_fairing = AdHoc::on_shutdown("Shutdown Leaderboard Sockets", |_rocket| {
            Box::pin(async move {
                tx.send(true).ok();
            })
        });

        let manager = LeaderboardManager::new(rx).await;
        rocket
            .attach(shutdown_fairing)
            .manage::<LeaderboardManagerHandle>(Arc::new(Mutex::new(manager)))
            .mount("/", routes![leaderboard_get, leaderboard_ws])
    })
}
