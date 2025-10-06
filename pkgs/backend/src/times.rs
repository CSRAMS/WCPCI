use chrono::{DateTime, Datelike, NaiveDateTime};
use chrono_tz::Tz;
use rocket::{
    fairing::AdHoc,
    form::{FromFormField, ValueField},
    request::{self, FromRequest},
    Request, State,
};
use serde::Serializer;

const HTML_FORMAT: &str = "%FT%R";

pub fn naive_to_html_time(dt: NaiveDateTime) -> String {
    dt.format(HTML_FORMAT).to_string()
}

pub fn datetime_to_html_time(dt: &DateTime<Tz>) -> String {
    dt.format(HTML_FORMAT).to_string()
}

pub fn format_datetime_human_readable(dt: DateTime<Tz>) -> String {
    let current_year = chrono::offset::Utc::now().year();
    let fstring = if dt.year() == current_year {
        "%a %B %-d %I:%M %p"
    } else {
        "%a %B %-d %Y %I:%M %p"
    };
    dt.format(fstring).to_string()
}

pub fn serialize_to_js<S: Serializer>(
    dt: &NaiveDateTime,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    serializer.serialize_str(&naive_to_html_time(*dt))
}

#[derive(Debug, Clone)]
pub struct FormDateTime(pub NaiveDateTime);

impl<'r> FromFormField<'r> for FormDateTime {
    fn from_value(field: ValueField<'r>) -> rocket::form::Result<'r, Self> {
        let dt = NaiveDateTime::parse_from_str(field.value, HTML_FORMAT);
        if let Ok(dt) = dt {
            Ok(FormDateTime(dt))
        } else {
            Err(rocket::form::Error::validation("Invalid date time").into())
        }
    }
}

#[derive(Debug)]
pub struct ClientTimeZone(Tz);

impl ClientTimeZone {
    pub fn timezone(&self) -> &Tz {
        &self.0
    }
}

#[derive(Debug)]
pub struct DefaultTimeZone(pub Tz);

#[rocket::async_trait]
impl<'r> FromRequest<'r> for ClientTimeZone {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let timezone = req
            .local_cache_async(async {
                let default_tz = req
                    .guard::<&State<DefaultTimeZone>>()
                    .await
                    .succeeded()
                    .map(|d| d.0)
                    .unwrap_or(Tz::UTC);
                req.cookies()
                    .get("timezone")
                    .and_then(|c| c.value().to_string().parse::<Tz>().ok())
                    .unwrap_or(default_tz)
            })
            .await;
        rocket::outcome::Outcome::Success(ClientTimeZone(*timezone))
    }
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("Timezone", |rocket| async {
        let default_tz = rocket
            .figment()
            .extract_inner::<String>("timezone")
            .ok()
            .and_then(|raw| raw.parse().ok())
            .unwrap_or(Tz::UTC);
        rocket.manage(DefaultTimeZone(default_tz))
    })
}
