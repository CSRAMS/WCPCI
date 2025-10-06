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

use super::{Contest, Participant};

#[get("/<contest_id>")]
pub async fn view_contest(
    mut db: DbConnection,
    contest_id: i64,
    tz: ClientTimeZone,
    user: Option<&User>,
    admin: Option<&Admin>,
) -> ResultResponse<Template> {
    let contest = Contest::get_or_404(&mut db, contest_id).await?;
    let participant = if let Some(user) = user {
        Participant::get(&mut db, contest_id, user.id).await?
    } else {
        None
    };

    let problems = Problem::list(&mut db, contest_id).await?;

    let (participants, judges) = Participant::list(&mut db, contest_id)
        .await?
        .into_iter()
        .partition::<Vec<_>, _>(|p| !p.0.is_judge);

    let start_local = tz.timezone().from_utc_datetime(&contest.start_time);
    let start_local_html = datetime_to_html_time(&start_local);
    let end_local = tz.timezone().from_utc_datetime(&contest.end_time);

    let start_formatted = format_datetime_human_readable(start_local);
    let end_formatted = format_datetime_human_readable(end_local);
    let tz_name = tz.timezone().name();

    let can_edit = admin.is_some() || participant.as_ref().is_some_and(|p| p.is_judge);

    let ctx = context_with_base!(
        user,
        problems,
        participants,
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
        participant
    );
    Ok(Template::render("contests/view", ctx))
}
