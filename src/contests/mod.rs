#![allow(clippy::blocks_in_conditions)] // Needed for the derive of FromForm, rocket is weird

use std::collections::HashMap;

use chrono::{NaiveDateTime, TimeZone};
use rocket::{fairing::AdHoc, form, http::Status, routes, FromForm};
use serde::Serialize;

use crate::{
    auth::users::{Admin, User},
    db::DbPoolConnection,
    error::prelude::*,
    template::TemplatedForm,
    times::{datetime_to_html_time, ClientTimeZone, FormDateTime},
};

mod admin;
mod delete;
mod edit;
mod git;
mod join;
mod judge;
mod list;
mod new;
mod team;
mod view;

pub use judge::Judge;
pub use team::{Team, TeamMember};

#[derive(Serialize, Clone)]
pub struct Contest {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    #[serde(serialize_with = "crate::times::serialize_to_js")]
    pub start_time: NaiveDateTime,
    #[serde(serialize_with = "crate::times::serialize_to_js")]
    pub registration_deadline: NaiveDateTime,
    #[serde(serialize_with = "crate::times::serialize_to_js")]
    pub end_time: NaiveDateTime,
    pub freeze_time: i64,
    pub penalty: i64,
    max_participants: Option<i64>,
    created_at: Option<NaiveDateTime>,
}

impl Contest {
    #[allow(clippy::too_many_arguments)]
    pub fn temp(
        name: String,
        description: Option<String>,
        start_time: NaiveDateTime,
        registration_deadline: NaiveDateTime,
        end_time: NaiveDateTime,
        freeze_time: i64,
        penalty: i64,
        max_participants: Option<i64>,
    ) -> Self {
        Self {
            id: 0,
            name,
            description,
            start_time,
            registration_deadline,
            end_time,
            freeze_time,
            penalty,
            max_participants,
            created_at: None,
        }
    }

    pub async fn list(db: &mut DbPoolConnection) -> Result<Vec<Self>> {
        sqlx::query_as!(Contest, "SELECT * FROM contest ORDER BY created_at DESC")
            .fetch_all(&mut **db)
            .await
            .context("Error fetching contests")
    }

    pub async fn list_user_in(db: &mut DbPoolConnection, user_id: i64) -> Result<Vec<Self>> {
        sqlx::query_as!(Contest, "SELECT contest.* FROM contest JOIN team ON contest.id = team.contest_id JOIN team_member ON team.id = team_member.id WHERE team_member.user_id = ?", user_id)
            .fetch_all(&mut **db)
            .await.context("Error fetching contests user is in")
    }

    pub async fn list_teams(&self, db: &mut DbPoolConnection) -> Result<Vec<team::Team>> {
        sqlx::query_as!(
            team::Team,
            "SELECT * FROM team WHERE contest_id = ?",
            self.id
        )
        .fetch_all(&mut **db)
        .await
        .context("Error fetching teams")
    }

    pub async fn list_judges(&self, db: &mut DbPoolConnection) -> Result<Vec<User>> {
        sqlx::query_as!(User, "SELECT user.* FROM user JOIN judge ON user.id = judge.user_id WHERE judge.contest_id = ?", self.id)
            .fetch_all(&mut **db)
            .await
            .context("Error fetching judges")
    }

    pub async fn get(db: &mut DbPoolConnection, id: i64) -> Result<Option<Self>> {
        sqlx::query_as!(Contest, "SELECT * FROM contest WHERE id = ?", id)
            .fetch_optional(&mut **db)
            .await
            .context("Error fetching contest")
    }

    pub async fn get_or_404(db: &mut DbPoolConnection, id: i64) -> ResultResponse<Self> {
        Self::get(db, id).await?.ok_or(Status::NotFound.into())
    }

    pub async fn get_or_404_assert_can_edit(
        db: &mut DbPoolConnection,
        id: i64,
        user: &User,
        admin: Option<&Admin>,
    ) -> ResultResponse<Self> {
        let contest = Self::get_or_404(db, id).await?;
        let is_judge = Judge::for_contest(id, user.id, db).await?.is_some();
        if !is_judge && admin.is_none() {
            Err(Status::Forbidden.into())
        } else {
            Ok(contest)
        }
    }

    pub async fn get_or_404_assert_started(
        db: &mut DbPoolConnection,
        id: i64,
        user: Option<&User>,
        admin: Option<&Admin>,
    ) -> ResultResponse<(Self, bool)> {
        let contest = Self::get_or_404(db, id).await?;
        let judge = if let Some(user) = user {
            Judge::for_contest(id, user.id, db).await?
        } else {
            None
        };
        let can_edit = admin.is_some() || judge.is_some();
        let started = contest.has_started();
        if !started && !can_edit {
            Err(Status::Forbidden.into())
        } else {
            Ok((contest, admin.is_some()))
        }
    }

    pub async fn insert(&self, db: &mut DbPoolConnection) -> Result<Self> {
        sqlx::query_as!(
            Contest,
            "INSERT INTO contest (name, description, start_time, registration_deadline, end_time, freeze_time, penalty, max_participants) VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING *",
            self.name,
            self.description,
            self.start_time,
            self.registration_deadline,
            self.end_time,
            self.freeze_time,
            self.penalty,
            self.max_participants
        ).fetch_one(&mut **db).await.context("Error inserting contest")
    }

    pub async fn update(&self, db: &mut DbPoolConnection) -> Result {
        sqlx::query_as!(
            Contest,
            "UPDATE contest SET name = ?, description = ?, start_time = ?, registration_deadline = ?, end_time = ?, freeze_time = ?, penalty = ?, max_participants = ? WHERE id = ?",
            self.name,
            self.description,
            self.start_time,
            self.registration_deadline,
            self.end_time,
            self.freeze_time,
            self.penalty,
            self.max_participants,
            self.id
        ).execute(&mut **db).await.map(|_| ()).with_context(|| format!("Error updating contest with id: {}", self.id))
    }

    pub async fn delete(self, db: &mut DbPoolConnection) -> Result {
        sqlx::query!("DELETE FROM contest WHERE id = ?", self.id)
            .execute(&mut **db)
            .await
            .map(|_| ())
            .with_context(|| format!("Error deleting contest {}", self.id))
    }

    pub fn has_started(&self) -> bool {
        let now = chrono::offset::Utc::now().naive_utc();
        self.start_time < now
    }

    pub fn has_ended(&self) -> bool {
        let now = chrono::offset::Utc::now().naive_utc();
        self.end_time < now
    }

    pub fn is_frozen(&self) -> bool {
        if self.freeze_time == 0 {
            return false;
        }
        let now = chrono::offset::Utc::now().naive_utc();
        let freeze_time_utc = self.end_time - chrono::Duration::minutes(self.freeze_time);
        freeze_time_utc < now && self.end_time > now
    }

    pub fn is_running(&self) -> bool {
        let now = chrono::offset::Utc::now().naive_utc();
        self.start_time < now && self.end_time > now
    }

    pub fn can_register(&self) -> bool {
        let now = chrono::offset::Utc::now().naive_utc();
        self.registration_deadline > now
    }

    pub fn progress(&self) -> usize {
        let now = chrono::offset::Utc::now().naive_utc();
        let total_seconds = (self.end_time - self.start_time).num_seconds() as f64;
        let elapsed_seconds = (now - self.start_time).num_seconds() as f64;
        (elapsed_seconds / total_seconds * 100.0) as usize
    }

    pub fn freeze_percent(&self) -> usize {
        let total_minutes = (self.end_time - self.start_time).num_minutes() as f64;
        let freeze_minutes = total_minutes - (self.freeze_time as f64);
        (freeze_minutes / total_minutes * 100.0) as usize
    }
}

struct ContestFormTemplate<'r> {
    contest: Option<&'r Contest>,
    judges: &'r Vec<User>,
    timezone: &'r ClientTimeZone,
}

impl<'r> TemplatedForm for ContestFormTemplate<'r> {
    fn get_defaults(&mut self) -> std::collections::HashMap<String, String> {
        if let Some(contest) = self.contest {
            let mut map = HashMap::from_iter([
                ("name".to_string(), contest.name.to_string()),
                (
                    "description".to_string(),
                    contest
                        .description
                        .as_ref()
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| "".to_string()),
                ),
                (
                    "start_time".to_string(),
                    datetime_to_html_time(
                        &self
                            .timezone
                            .timezone()
                            .from_utc_datetime(&contest.start_time),
                    ),
                ),
                (
                    "registration_deadline".to_string(),
                    datetime_to_html_time(
                        &self
                            .timezone
                            .timezone()
                            .from_utc_datetime(&contest.registration_deadline),
                    ),
                ),
                (
                    "end_time".to_string(),
                    datetime_to_html_time(
                        &self
                            .timezone
                            .timezone()
                            .from_utc_datetime(&contest.end_time),
                    ),
                ),
                ("freeze_time".to_string(), contest.freeze_time.to_string()),
                ("penalty".to_string(), contest.penalty.to_string()),
                (
                    "max_participants".to_string(),
                    contest
                        .max_participants
                        .map(|i| i.to_string())
                        .unwrap_or("null".to_string()),
                ),
            ]);
            for judge in self.judges.iter() {
                map.insert(format!("judges[{}]", judge.id), "true".to_string());
            }
            map
        } else {
            HashMap::from_iter([
                ("name".to_string(), "".to_string()),
                ("description".to_string(), "".to_string()),
                ("start_time".to_string(), String::new()),
                ("registration_deadline".to_string(), String::new()),
                ("end_time".to_string(), String::new()),
                ("freeze_time".to_string(), "0".to_string()),
                ("penalty".to_string(), "30".to_string()),
                ("max_participants".to_string(), "".to_string()),
            ])
        }
    }
}

#[inline]
fn over_1<'e>(max_participants: &Option<i64>) -> Result<(), rocket::form::Errors<'e>> {
    if let Some(i) = max_participants {
        if *i > 0 {
            Ok(())
        } else {
            Err(form::Error::validation("Must be over 1").into())
        }
    } else {
        Ok(())
    }
}

#[inline]
fn len_under_1000<'r, 'e>(s: &'r Option<&'r str>) -> Result<(), rocket::form::Errors<'e>> {
    if let Some(s) = s {
        if s.len() < 1000 {
            Ok(())
        } else {
            Err(form::Error::validation("Must be under 1000 characters").into())
        }
    } else {
        Ok(())
    }
}

#[inline]
fn within_bound<'r, 'e>(
    freeze_time: &'r i64,
    end_time: &'r NaiveDateTime,
    start_time: &'r NaiveDateTime,
) -> Result<(), rocket::form::Errors<'e>> {
    let freeze_time_utc = *end_time - chrono::Duration::minutes(*freeze_time);
    if freeze_time_utc > *start_time {
        Ok(())
    } else {
        Err(form::Error::validation("This will result in the contest being frozen before the contest starts, please choose a different time").into())
    }
}

#[derive(FromForm)]
struct ContestForm<'r> {
    #[field(validate = len(1..=100))]
    name: &'r str,
    #[field(validate = len_under_1000())]
    description: Option<&'r str>,
    start_time: FormDateTime,
    registration_deadline: FormDateTime,
    end_time: FormDateTime,
    #[field(validate = range(0..))]
    #[field(validate = within_bound(&self.end_time.0, &self.start_time.0))]
    freeze_time: i64,
    #[field(validate = range(0..))]
    penalty: i64,
    #[field(validate = over_1())]
    max_participants: Option<i64>,
    judges: HashMap<i64, bool>,
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("Contests App", |rocket| async {
        rocket.attach(admin::stage()).attach(git::stage()).mount(
            "/contests",
            routes![
                list::contests_list,
                new::new_contest_get,
                new::new_contest_post,
                edit::edit_contest_get,
                edit::edit_contest_post,
                delete::delete_contest_get,
                delete::delete_contest_post,
                join::join_contest,
                view::view_contest,
            ],
        )
    })
}
