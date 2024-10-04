use chrono::TimeZone;
use rocket::get;
use rocket_dyn_templates::Template;

use crate::{
    auth::users::{Admin, User},
    context_with_base,
    db::DbConnection,
    error::prelude::*,
    problems::Problem,
    times::{datetime_to_html_time, format_datetime_human_readable, ClientTimeZone},
};

use super::{Contest, Judge, Team};

#[get("/<contest_id>")]
pub async fn view_contest(
    mut db: DbConnection,
    contest_id: i64,
    tz: ClientTimeZone,
    user: Option<&User>,
    admin: Option<&Admin>,
) -> ResultResponse<Template> {
    let contest = Contest::get_or_404(&mut db, contest_id).await?;
    let team = if let Some(user) = user {
        Team::from_user_and_contest(&mut db, user.id, contest_id).await?
    } else {
        None
    };

    let problems = Problem::list(&mut db, contest_id).await?;

    let teams = Team::list(&mut db, contest_id).await?;
    let judges = Judge::all_for_contest(contest_id, &mut db).await?;

    let is_judge = user.map(|u| judges.iter().any(|j| j.id == u.id)).unwrap_or(false);

    let start_local = tz.timezone().from_utc_datetime(&contest.start_time);
    let start_local_html = datetime_to_html_time(&start_local);
    let end_local = tz.timezone().from_utc_datetime(&contest.end_time);

    let start_formatted = format_datetime_human_readable(start_local);
    let end_formatted = format_datetime_human_readable(end_local);
    let tz_name = tz.timezone().name();

    let can_edit = admin.is_some() || is_judge;

    // TODO: contest template needs to be updated
    let ctx = context_with_base!(
        user,
        problems,
        teams,
        team,
        tz_name,
        can_edit,
        start_formatted,
        start_local_html,
        end_formatted,
        is_admin: admin.is_some(),
        judges,
        started: contest.has_started(),
        ended: contest.has_ended(),
        contest,
    );
    Ok(Template::render("contests/view", ctx))
}
