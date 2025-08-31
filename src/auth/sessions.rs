use chrono::NaiveDateTime;
use rand::{distr::Alphanumeric, Rng};

use crate::{db::DbPoolConnection, error::prelude::*};

pub struct Session {
    pub id: i64,
    // For some reason these are marked as unused? sqlx stuff i guess
    #[allow(dead_code)]
    pub user_id: i64,
    #[allow(dead_code)]
    pub token: String,
    #[allow(dead_code)]
    pub created_at: NaiveDateTime,
    pub expires_at: NaiveDateTime,
}

impl Session {
    pub const TOKEN_COOKIE_NAME: &'static str = "token";
    const TOKEN_LENGTH: usize = 64;
    const EXPIRY_DAYS: i64 = 14;

    fn gen_token() -> String {
        rand::rng()
            .sample_iter(&Alphanumeric)
            .take(Self::TOKEN_LENGTH)
            .map(char::from)
            .collect()
    }

    pub fn hash_token(token: &str) -> String {
        sha256::digest(token)
    }

    pub async fn create(db: &mut DbPoolConnection, user_id: i64) -> Result<(Session, String)> {
        let token = Self::gen_token();
        let now = chrono::offset::Utc::now();
        let expires = now
            + chrono::TimeDelta::try_days(Self::EXPIRY_DAYS)
                .context("Failed to set expiry days")?;
        let hash = Self::hash_token(&token);
        let session = sqlx::query_as!(Session, "INSERT INTO session (user_id, token, created_at, expires_at) VALUES (?, ?, ?, ?) RETURNING *", user_id, hash, now, expires)
            .fetch_one(&mut **db).await.context("Couldn't insert new session")?;

        Ok((session, token))
    }

    pub async fn from_token(db: &mut DbPoolConnection, token: &str) -> Result<Option<Session>> {
        let hash = Self::hash_token(token);
        sqlx::query_as!(
            Session,
            "SELECT * FROM session WHERE session.token = ? AND expires_at > CURRENT_TIMESTAMP",
            hash
        )
        .fetch_optional(&mut **db)
        .await
        .context("Couldn't fetch session by token")
    }
}
