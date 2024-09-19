use chrono::NaiveDateTime;

use crate::{db::DbPoolConnection, error::prelude::*};

#[derive(Serialize, Debug)]
pub struct ProblemCompletion {
    pub team_id: i64,
    pub problem_id: i64,
    pub completed_at: Option<NaiveDateTime>,
    pub number_wrong: i64,
}

impl ProblemCompletion {
    pub async fn upsert(&self, db: &mut DbPoolConnection) -> Result {
        sqlx::query_as!(
            ProblemCompletion,
            "INSERT OR REPLACE INTO problem_completion (team_id, problem_id, completed_at, number_wrong) VALUES (?, ?, ?, ?)",
            self.team_id,
            self.problem_id,
            self.completed_at,
            self.number_wrong
        )
        .execute(&mut **db)
        .await.map(|_| ()).context("Failed to upsert problem completion")
    }

    pub async fn get_for_problem_and_team(
        db: &mut DbPoolConnection,
        problem_id: i64,
        team_id: i64,
    ) -> Result<Option<Self>> {
        sqlx::query_as!(
            ProblemCompletion,
            "SELECT * FROM problem_completion WHERE team_id = ? AND problem_id = ?",
            team_id,
            problem_id
        )
        .fetch_optional(&mut **db)
        .await
        .with_context(|| {
            format!(
                "Failed to get problem completion for problem {} and participant {}",
                problem_id, team_id
            )
        })
    }

    pub async fn get_for_team(db: &mut DbPoolConnection, team_id: i64) -> Result<Vec<Self>> {
        sqlx::query_as!(
            ProblemCompletion,
            "SELECT * FROM problem_completion WHERE team_id = ?",
            team_id,
        )
        .fetch_all(&mut **db)
        .await
        .with_context(|| format!("Failed to get problem completions for team {}", team_id))
    }

    pub fn temp(team_id: i64, problem_id: i64, completed_at: Option<NaiveDateTime>) -> Self {
        Self {
            team_id,
            problem_id,
            completed_at,
            number_wrong: 0,
        }
    }
}
