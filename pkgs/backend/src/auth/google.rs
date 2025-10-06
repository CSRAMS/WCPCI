use super::prelude::*;

pub struct GoogleLogin(pub String);

const SCOPES: [&str; 2] = [
    "https://www.googleapis.com/auth/userinfo.email",
    "https://www.googleapis.com/auth/userinfo.profile",
];

#[derive(Debug, serde::Deserialize)]
pub struct UserInfo {
    pub user_id: String,
}

#[rocket::async_trait]
impl CallbackHandler for GoogleLogin {
    type IntermediateUserInfo = UserInfo;

    const SERVICE_NAME: &'static str = "Google";

    fn get_request_client(&self) -> reqwest::RequestBuilder {
        reqwest::Client::new()
            .get(format!(
                "https://www.googleapis.com/oauth2/v1/tokeninfo?access_token={}",
                self.0
            ))
            .header("User-Agent", "Test-App")
            .header("Accept", "application/json")
            .header("Authorization", format!("Bearer {}", self.0))
    }

    async fn get_user(
        &self,
        db: &mut DbPoolConnection,
        user: Self::IntermediateUserInfo,
    ) -> Result<Option<User>> {
        let res = sqlx::query_as!(User, "SELECT * FROM user WHERE google_id = ?", user.user_id)
            .fetch_optional(&mut **db)
            .await?;
        Ok(res)
    }

    async fn link_to(
        &self,
        db: &mut DbPoolConnection,
        user: &User,
        user_info: Self::IntermediateUserInfo,
    ) -> Result<bool> {
        let other_exists = sqlx::query!(
            "SELECT * FROM user WHERE google_id = ? AND id != ?",
            user_info.user_id,
            user.id
        )
        .fetch_optional(&mut **db)
        .await?
        .is_some();

        if other_exists {
            return Ok(false);
        }

        let res = sqlx::query!(
            "UPDATE user SET google_id = ? WHERE id = ?",
            user_info.user_id,
            user.id
        )
        .execute(&mut **db)
        .await
        .map(|r| r.rows_affected() == 1)?;
        Ok(res)
    }

    async fn unlink(db: &mut DbPoolConnection, user: &User) -> Result<SqliteQueryResult> {
        sqlx::query!("UPDATE user SET github_id = NULL WHERE id = ?", user.id)
            .execute(&mut **db)
            .await
            .context("Error unlinking Google account")
    }
}

oauth_fairing!("Google", google, GoogleLogin, SCOPES);
