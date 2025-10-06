use chrono::TimeZone;
use rocket::get;
use rocket_dyn_templates::Template;

use crate::{
    auth::users::{Admin, User},
    context_with_base,
    db::DbConnection,
    error::prelude::*,
    times::{format_datetime_human_readable, ClientTimeZone},
};

use super::Contest;

#[get("/")]
pub async fn contests_list(
    user: Option<&User>,
    admin: Option<&Admin>,
    timezone: ClientTimeZone,
    mut db: DbConnection,
) -> ResultResponse<Template> {
    let contests = Contest::list(&mut db).await?;
    let tz = timezone.timezone();
    let start_times = contests
        .iter()
        .map(|c| format_datetime_human_readable(tz.from_utc_datetime(&c.start_time)))
        .collect::<Vec<_>>();
    let registration_deadlines = contests
        .iter()
        .map(|c| format_datetime_human_readable(tz.from_utc_datetime(&c.registration_deadline)))
        .collect::<Vec<_>>();
    let ctx = context_with_base!(user, contests, start_times, registration_deadlines, is_admin: admin.is_some());
    Ok(Template::render("contests/list", ctx))
}
