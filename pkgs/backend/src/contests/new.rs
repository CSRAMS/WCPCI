use chrono::TimeZone;
use rocket::{
    form::{Contextual, Form},
    get, post,
};
use rocket_dyn_templates::Template;

use crate::{
    auth::{
        csrf::{CsrfToken, VerifyCsrfToken},
        users::{Admin, User},
    },
    contests::ContestForm,
    context_with_base_authed,
    db::DbConnection,
    messages::Message,
    template::FormTemplateObject,
    times::ClientTimeZone,
    FormResponse,
};

use super::{Contest, ContestFormTemplate, Participant};

#[get("/new")]
pub async fn new_contest_get(
    mut db: DbConnection,
    user: &User,
    _admin: &Admin,
    timezone: ClientTimeZone,
    _token: &CsrfToken,
) -> Template {
    let form_template = ContestFormTemplate {
        contest: None,
        judges: &Vec::new(),
        timezone: &timezone,
    };
    let all_users = User::list(&mut db).await.unwrap_or_default();
    let form = FormTemplateObject::get(form_template);
    let ctx = context_with_base_authed!(user, all_users, judges: Vec::<String>::new(), form);
    Template::render("contests/new", ctx)
}

#[post("/new", data = "<form>")]
pub async fn new_contest_post(
    mut db: DbConnection,
    user: &User,
    timezone: ClientTimeZone,
    _admin: &Admin,
    _token: &VerifyCsrfToken,
    form: Form<Contextual<'_, ContestForm<'_>>>,
) -> FormResponse {
    if let Some(ref value) = form.value {
        let tz = timezone.timezone();

        let name = value.name.to_string();
        let description = value.description.as_ref().map(|s| s.to_string());
        let start_time = tz
            .from_local_datetime(&value.start_time.0)
            .unwrap()
            .naive_utc();
        let registration_deadline = tz
            .from_local_datetime(&value.registration_deadline.0)
            .unwrap()
            .naive_utc();
        let end_time = tz
            .from_local_datetime(&value.end_time.0)
            .unwrap()
            .naive_utc();
        let freeze_time = value.freeze_time;
        let penalty = value.penalty;
        let max_participants = value.max_participants;
        let contest = Contest::temp(
            name,
            description,
            start_time,
            registration_deadline,
            end_time,
            freeze_time,
            penalty,
            max_participants,
        );
        let contest = contest.insert(&mut db).await?;
        for judge in value.judges.keys() {
            Participant::create_or_make_judge(&mut db, contest.id, *judge).await?;
        }
        Ok(Message::success("Contest Created").to(&format!("/contests/{}", contest.id)))
    } else {
        let form_template = ContestFormTemplate {
            contest: None,
            judges: &Vec::new(),
            timezone: &timezone,
        };
        let form = FormTemplateObject::from_rocket_context(form_template, &form.context);
        let ctx = context_with_base_authed!(user, form);
        Err(Template::render("contests/new", ctx).into())
    }
}
