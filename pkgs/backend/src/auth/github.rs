use super::prelude::*;

pub struct GitHubLogin(pub String);

const SCOPES: [&str; 1] = ["user:read"];

#[derive(Debug, serde::Deserialize)]
pub struct UserInfo {
    pub id: i64,
}

#[rocket::async_trait]
impl CallbackHandler for GitHubLogin {
    type IntermediateUserInfo = UserInfo;

    const SERVICE_NAME: &'static str = "GitHub";

    fn get_request_client(&self) -> reqwest::RequestBuilder {
        reqwest::Client::new()
            .get("https://api.github.com/user")
            .header("User-Agent", "Test-App")
            .header(
                "Accept",
                "application/vnd.github+json,application/vnd.github.diff",
            )
            .header("X-GitHub-Api-Version", "2022-11-28")
            .header("Authorization", format!("Bearer {}", self.0))
    }

    async fn get_user(
        &self,
        db: &mut DbPoolConnection,
        user: Self::IntermediateUserInfo,
    ) -> Result<Option<User>> {
        let res = sqlx::query_as!(User, "SELECT * FROM user WHERE github_id = ?", user.id)
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
            "SELECT * FROM user WHERE github_id = ? AND id != ?",
            user_info.id,
            user.id
        )
        .fetch_optional(&mut **db)
        .await?
        .is_some();

        if other_exists {
            return Ok(false);
        }

        let res = sqlx::query!(
            "UPDATE user SET github_id = ? WHERE id = ?",
            user_info.id,
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
            .context("Error unlinking GitHub account")
    }
}

oauth_fairing!("GitHub", github, GitHubLogin, SCOPES);
