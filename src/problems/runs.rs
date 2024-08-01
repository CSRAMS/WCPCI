use chrono::NaiveDateTime;
use chrono::TimeZone;
use rocket::get;
use rocket_dyn_templates::Template;

use crate::auth::users::Admin;
use crate::auth::users::User;
use crate::contests::Contest;
use crate::contests::Participant;
use crate::context_with_base;
use crate::db::{DbConnection, DbPoolConnection};
use crate::error::prelude::*;
use crate::run::JobState;
use crate::times::format_datetime_human_readable;
use crate::times::ClientTimeZone;

use super::Problem;

#[derive(Debug, Serialize)]
pub struct JudgeRun {
    pub id: i64,
    pub problem_id: i64,
    pub user_id: i64,
    pub amount_run: i64,
    pub program: String,
    pub language: String,
    pub total_cases: i64,
    pub error: Option<String>,
    #[serde(serialize_with = "crate::times::serialize_to_js")]
    pub ran_at: NaiveDateTime,
}

impl JudgeRun {
    #[allow(clippy::too_many_arguments)]
    pub fn temp(
        problem_id: i64,
        user_id: i64,
        amount_run: i64,
        program: String,
        language: String,
        total_cases: i64,
        error: Option<String>,
        ran_at: NaiveDateTime,
    ) -> Self {
        Self {
            id: 0,
            problem_id,
            user_id,
            amount_run,
            program,
            language,
            total_cases,
            error,
            ran_at,
        }
    }

    pub fn from_job_state(
        problem_id: i64,
        user_id: i64,
        program: String,
        language: String,
        state: &JobState,
        ran_at: NaiveDateTime,
    ) -> Self {
        let (amount_run, _, error) = state.last_error();
        Self::temp(
            problem_id,
            user_id,
            amount_run as i64,
            program,
            language,
            state.len() as i64,
            error,
            ran_at,
        )
    }

    pub async fn list(
        db: &mut DbPoolConnection,
        user_id: i64,
        problem_id: i64,
        limit: i64,
    ) -> Result<Vec<Self>> {
        sqlx::query_as!(
            JudgeRun,
            "SELECT * FROM judge_run WHERE user_id = ? AND problem_id = ? ORDER BY ran_at DESC LIMIT ?",
            user_id,
            problem_id,
            limit
        )
        .fetch_all(&mut **db)
        .await
        .with_context(|| format!("Failed to get runs for user {} and problem {}", user_id, problem_id))
    }

    pub async fn get_latest(
        db: &mut DbPoolConnection,
        user_id: i64,
        problem_id: i64,
    ) -> Result<Option<Self>> {
        sqlx::query_as!(
            JudgeRun,
            "SELECT * FROM judge_run WHERE user_id = ? AND problem_id = ? ORDER BY ran_at DESC LIMIT 1",
            user_id,
            problem_id
        )
            .fetch_optional(&mut **db)
            .await
            .with_context(|| format!("Failed to get latest run for user {} and problem {}", user_id, problem_id))
    }

    pub async fn get_latest_success(
        db: &mut DbPoolConnection,
        user_id: i64,
        problem_id: i64,
    ) -> Result<Option<Self>> {
        sqlx::query_as!(
            JudgeRun,
            "SELECT * FROM judge_run WHERE user_id = ? AND problem_id = ? AND amount_run = total_cases AND error IS NULL ORDER BY ran_at DESC LIMIT 1",
            user_id,
            problem_id
        )
            .fetch_optional(&mut **db)
            .await
            .with_context(|| format!("Failed to get latest successful run for user {} and problem {}", user_id, problem_id))
    }

    pub const MAX_RUNS_PER_USER: i64 = 25;

    pub async fn write_to_db(self, db: &mut DbPoolConnection) -> Result<Self> {
        let new = sqlx::query_as!(
            JudgeRun,
            "INSERT INTO judge_run (problem_id, user_id, amount_run, program, language, total_cases, error, ran_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?) RETURNING *",
            self.problem_id,
            self.user_id,
            self.amount_run,
            self.program,
            self.language,
            self.total_cases,
            self.error,
            self.ran_at
        )
            .fetch_one(&mut **db)
            .await.context("Failed to insert new run")?;

        let run_count = sqlx::query!(
            "SELECT * FROM judge_run WHERE user_id = ? AND problem_id = ?",
            self.user_id,
            self.problem_id
        )
        .fetch_all(&mut **db)
        .await
        .context("Failed to count runs for user")?
        .len() as i64;

        if run_count > Self::MAX_RUNS_PER_USER {
            sqlx::query!(
                "DELETE FROM judge_run WHERE id = (SELECT id FROM judge_run WHERE user_id = ? AND problem_id = ? ORDER BY ran_at ASC LIMIT 1)",
                self.user_id,
                self.problem_id
            )
                .execute(&mut **db)
                .await
                .context("Failed to delete oldest run")?;
        }

        Ok(new)
    }

    pub fn success(&self) -> bool {
        self.amount_run == self.total_cases && self.error.is_none()
    }
}

#[get("/<contest_id>/problems/<slug>/runs")]
pub async fn runs(
    contest_id: i64,
    slug: &str,
    tz: ClientTimeZone,
    admin: Option<&Admin>,
    user: Option<&User>,
    mut db: DbConnection,
) -> ResultResponse<Template> {
    let problem = Problem::get_or_404(&mut db, contest_id, slug).await?;
    let contest = Contest::get_or_404(&mut db, contest_id).await?;
    let runs = if let Some(user) = user {
        JudgeRun::list(&mut db, user.id, problem.id, JudgeRun::MAX_RUNS_PER_USER).await?
    } else {
        vec![]
    };
    let participant = if let Some(user) = user {
        Participant::get(&mut db, contest_id, user.id).await?
    } else {
        None
    };
    let can_edit = admin.is_some() || participant.map_or(false, |p| p.is_judge);
    let tz = tz.timezone();
    let formatted_times = runs
        .iter()
        .map(|r| tz.from_utc_datetime(&r.ran_at))
        .map(format_datetime_human_readable)
        .collect::<Vec<_>>();
    Ok(Template::render(
        "problems/runs",
        context_with_base!(user, runs, contest, problem, can_edit, formatted_times, max_runs: JudgeRun::MAX_RUNS_PER_USER),
    ))
}
