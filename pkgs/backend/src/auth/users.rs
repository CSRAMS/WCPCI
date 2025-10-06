use chrono::NaiveDateTime;
use rocket::{
    http::{Cookie, CookieJar, SameSite, Status},
    outcome::{IntoOutcome, Outcome},
    request::{self, FromRequest},
    time::OffsetDateTime,
    FromFormField, Request, State,
};
use serde::Serialize;
use sqlx::{encode::IsNull, prelude::FromRow, Decode, Encode, Type};

use crate::{
    db::{DbConnection, DbPoolConnection},
    error::prelude::*,
};

use super::sessions::Session;

#[derive(Debug, Clone, Serialize, FromFormField)]
pub enum ColorScheme {
    Light,
    Dark,
    UseSystem,
}

impl Default for ColorScheme {
    fn default() -> Self {
        Self::UseSystem
    }
}

impl From<String> for ColorScheme {
    fn from(s: String) -> Self {
        match s.as_str() {
            "Light" => Self::Light,
            "Dark" => Self::Dark,
            "UseSystem" => Self::UseSystem,
            _ => Self::UseSystem,
        }
    }
}

impl From<ColorScheme> for String {
    fn from(s: ColorScheme) -> Self {
        format!("{:?}", s)
    }
}

impl Type<sqlx::Sqlite> for ColorScheme {
    fn type_info() -> <sqlx::Sqlite as sqlx::Database>::TypeInfo {
        <String as Type<sqlx::Sqlite>>::type_info()
    }
}

impl Encode<'_, sqlx::Sqlite> for ColorScheme {
    fn encode_by_ref(
        &self,
        buf: &mut <sqlx::Sqlite as sqlx::database::HasArguments<'_>>::ArgumentBuffer,
    ) -> IsNull {
        let val = format!("{:?}", self);
        <std::string::String as Encode<'_, sqlx::Sqlite>>::encode_by_ref(&val, buf)
    }
}

impl Decode<'_, sqlx::Sqlite> for ColorScheme {
    fn decode(
        value: <sqlx::Sqlite as sqlx::database::HasValueRef<'_>>::ValueRef,
    ) -> std::result::Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let s = <String as Decode<sqlx::Sqlite>>::decode(value)?;
        Ok(s.into())
    }
}

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct User {
    pub id: i64,
    pub sso_id: String,
    pub email: String,
    pub bio: String,
    pub default_display_name: String,
    pub display_name: Option<String>,
    pub color_scheme: ColorScheme,
    pub default_language: String,
    pub created_at: NaiveDateTime,
    pub profile_picture_source: String,
    pub github_id: Option<i64>,
    pub google_id: Option<String>,
}

impl User {
    pub fn display_name(&self) -> &str {
        self.display_name
            .as_ref()
            .unwrap_or(&self.default_display_name)
    }

    pub fn temporary(
        id: String,
        email: String,
        display_name: String,
        default_language: &str,
    ) -> Self {
        Self {
            id: 0,
            sso_id: id,
            profile_picture_source: "gravatar".to_string(),
            bio: String::new(),
            email,
            default_display_name: display_name,
            color_scheme: ColorScheme::default(),
            default_language: default_language.to_string(),
            display_name: None,
            created_at: chrono::offset::Utc::now().naive_utc(),
            github_id: None,
            google_id: None,
        }
    }

    pub async fn login(&self, db: &mut DbPoolConnection, cookies: &CookieJar<'_>) -> Result {
        let (session, token) = Session::create(db, self.id).await?;

        let expires =
            OffsetDateTime::from_unix_timestamp(session.expires_at.and_utc().timestamp()).unwrap();

        cookies.add_private(
            Cookie::build(("token", token))
                .same_site(SameSite::Lax)
                .expires(expires)
                .build(),
        );

        Ok(())
    }

    async fn register<'a>(
        self,
        db: &mut DbPoolConnection,
        cookies: &'a CookieJar<'a>,
    ) -> Result<User> {
        let user = self.insert(db).await?;
        user.login(db, cookies).await?;
        Ok(user)
    }

    pub async fn login_or_register<'a>(
        self,
        db: &mut DbPoolConnection,
        cookies: &'a CookieJar<'a>,
    ) -> Result<(User, bool)> {
        let existing = sqlx::query_as!(User, "SELECT * FROM user WHERE sso_id = ?", self.sso_id)
            .fetch_optional(&mut **db)
            .await
            .with_context(|| {
                format!("Failed to fetch user from db with sso_id = {}", self.sso_id)
            })?;

        if let Some(user) = existing {
            // Update the user's display name and email if they have changed
            if user.email != self.email || user.default_display_name != self.default_display_name {
                let res = sqlx::query!(
                    "UPDATE user SET email = ?, default_display_name = ? WHERE id = ?",
                    self.email,
                    self.default_display_name,
                    user.id
                )
                .execute(&mut **db)
                .await;

                res.context("Failed to update user info from SSO")?;
            }
            user.login(db, cookies).await?;
            Ok((user, false))
        } else {
            let user = self.register(db, cookies).await;
            user.map(|u| (u, true))
        }
    }

    pub async fn insert(self, db: &mut DbPoolConnection) -> Result<Self> {
        let new = sqlx::query_as!(
            User,
            "INSERT INTO user (sso_id, email, default_display_name, color_scheme, default_language) VALUES (?, ?, ?, ?, ?) RETURNING *",
            self.sso_id,
            self.email,
            self.default_display_name,
            self.color_scheme,
            self.default_language
        )
        .fetch_one(&mut **db)
        .await
        .with_context(|| format!("Failed to insert new user: {self:?}"))?;

        Ok(new)
    }

    pub async fn delete(&self, db: &mut DbPoolConnection) -> Result {
        let res = sqlx::query!("DELETE FROM user WHERE id = ?", self.id)
            .execute(&mut **db)
            .await;

        res.with_context(|| format!("Failed to delete user with id: {}", self.id))?;

        Ok(())
    }

    pub async fn get(db: &mut DbPoolConnection, id: i64) -> Result<Option<Self>> {
        sqlx::query_as!(User, "SELECT * FROM user WHERE id = ?", id)
            .fetch_optional(&mut **db)
            .await
            .with_context(|| format!("Couldn't fetch user with id {}", id))
    }

    pub async fn get_or_404(db: &mut DbPoolConnection, id: i64) -> ResultResponse<Self> {
        Self::get(db, id).await?.ok_or(Status::NotFound.into())
    }

    pub async fn list(db: &mut DbPoolConnection) -> Result<Vec<Self>> {
        let users: Vec<User> = sqlx::query_as!(User, "SELECT * FROM user")
            .fetch_all(&mut **db)
            .await
            .context("Failed to list all users")?;

        Ok(users)
    }
}

pub struct AdminUsers(pub Vec<String>);

pub struct Admin();

#[rocket::async_trait]
impl<'r> FromRequest<'r> for &'r User {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let user_result = req.local_cache_async(async {
            let mut db = req.guard::<DbConnection>().await.succeeded().ok_or_else(|| {
                error!("Failed to get db connection");
                Status::InternalServerError
            })?;
            if let Some(token) = req.cookies().get_private(Session::TOKEN_COOKIE_NAME).map(|c| c.value().to_string()) {
                let hash = Session::hash_token(&token);
                let res = sqlx::query_as!(
                    User,
                    "SELECT user.* FROM user JOIN session ON user.id = session.user_id WHERE session.token = ? AND expires_at > CURRENT_TIMESTAMP",
                    hash
                )
                .fetch_optional(&mut **db)
                .await.context("Couldn't fetch user by token");
                match res {
                    Ok(Some(user)) => Ok(user),
                    Ok(None) => Err(Status::Unauthorized),
                    Err(why) => {
                        error!("Internal server error: {:?}", why);
                        Err(Status::InternalServerError)
                    },
                }
            } else {
                Err(Status::Unauthorized)
            }
        }).await.as_ref();

        match user_result {
            Ok(user) => Outcome::Success(user),
            Err(status) => Outcome::Error((*status, ())),
        }
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for &'r Admin {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let admin_result = req
            .local_cache_async(async {
                let user = req.guard::<&User>().await.succeeded()?;
                let admin_users = req.guard::<&State<AdminUsers>>().await.succeeded()?;
                if admin_users.0.contains(&user.email) {
                    Some(Admin())
                } else {
                    None
                }
            })
            .await;
        admin_result.as_ref().or_error((Status::Forbidden, ()))
    }
}
