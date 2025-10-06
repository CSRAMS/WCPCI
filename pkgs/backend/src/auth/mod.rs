#![allow(clippy::blocks_in_conditions)] // Needed for the derive of FromForm, rocket is weird

use log::warn;
use rocket::{
    catch, catchers,
    fairing::AdHoc,
    get,
    http::{Cookie, CookieJar, SameSite, Status},
    response::Redirect,
    routes,
    time::Duration,
    Request,
};
use rocket_dyn_templates::Template;
use sqlx::sqlite::SqliteQueryResult;

use crate::{
    context_with_base,
    db::{DbConnection, DbPoolConnection},
    error::prelude::*,
    messages::Message,
    ResultResponse,
};

use self::{
    sessions::Session,
    users::{AdminUsers, User},
};

mod github;
mod google;
mod saml;

pub use saml::{SamlOptions, PREFERRED_SSO_BINDING};

pub mod csrf;
pub mod sessions;
pub mod users;

const LOGIN_URI: &str = "/auth/login";

#[catch(401)]
async fn unauthorized(req: &Request<'_>) -> Redirect {
    let path = req.uri().path();
    let msg = Message::info("You need to be logged in to access this page");
    if path == LOGIN_URI {
        msg.to("/")
    } else {
        msg.to_with_params(LOGIN_URI, vec![("redirect", path.to_string().as_str())])
    }
}

const REDIRECT_COOKIE_NAME: &str = "redirect_after_auth";

#[get("/login?<redirect>")]
async fn login(user: Option<&User>, redirect: Option<&str>, cookies: &CookieJar<'_>) -> Template {
    if let Some(redirect) = redirect {
        let mut cookie = Cookie::new(REDIRECT_COOKIE_NAME, redirect.to_string());
        cookie.set_same_site(SameSite::Lax);
        cookie.set_secure(false);
        cookie.set_max_age(Duration::minutes(5));
        cookies.add(cookie);
    }
    let ctx = context_with_base!(user,);
    Template::render("auth/login", ctx)
}

#[get("/logout")]
async fn logout(mut db: DbConnection, cookies: &CookieJar<'_>) -> ResultResponse<Redirect> {
    if let Some(token) = cookies
        .get_private(Session::TOKEN_COOKIE_NAME)
        .map(|c| c.value().to_string())
    {
        let session = Session::from_token(&mut db, &token)
            .await
            .with_context(|| format!("Couldn't get session with token: {token}"))?;
        if let Some(session) = session {
            sqlx::query!("DELETE FROM session WHERE id = ?", session.id)
                .execute(&mut **db)
                .await
                .map_err(|why| anyhow!("Failed to delete session {}: {why:?}", session.id))?;
        }

        cookies.remove_private(Session::TOKEN_COOKIE_NAME);
    }
    Ok(Message::success("Logged out").to("/"))
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("Auth App", |rocket| async {
        let admins: Vec<String> = rocket
            .figment()
            .extract_inner("admins")
            .unwrap_or_else(|_| {
                warn!("No admin user specified");
                Vec::new()
            });
        rocket
            .manage(AdminUsers(admins))
            .attach(saml::stage())
            .attach(github::stage())
            .attach(google::stage())
            .attach(csrf::stage())
            .register("/", catchers![unauthorized])
            .mount("/auth", routes![login, logout,])
    })
}

const STATE_COOKIE_NAME: &str = "state-oauth-type";
const LOGIN_STATE: &str = "login";
const LINK_STATE: &str = "link";

#[rocket::async_trait]
pub trait CallbackHandler {
    type IntermediateUserInfo: serde::de::DeserializeOwned + Sync + Send;
    const SERVICE_NAME: &'static str;

    fn get_request_client(&self) -> reqwest::RequestBuilder;

    async fn get_user(
        &self,
        db: &mut DbPoolConnection,
        user: Self::IntermediateUserInfo,
    ) -> Result<Option<User>>;

    fn put_login_cookie(cookies: &CookieJar<'_>) {
        let mut cookie = Cookie::new(STATE_COOKIE_NAME, LOGIN_STATE);
        cookie.set_secure(false);
        cookie.set_same_site(SameSite::Lax);
        cookies.add(cookie);
    }

    fn put_link_cookie(cookies: &CookieJar<'_>) {
        let mut cookie = Cookie::new(STATE_COOKIE_NAME, LINK_STATE);
        cookie.set_secure(false);
        cookie.set_same_site(SameSite::Lax);
        cookies.add(cookie);
    }

    async fn link_to(
        &self,
        db: &mut DbPoolConnection,
        user: &User,
        user_info: Self::IntermediateUserInfo,
    ) -> Result<bool>;

    async fn unlink(db: &mut DbPoolConnection, user: &User) -> Result<SqliteQueryResult>;

    async fn handle_callback(
        &self,
        user: Option<&User>,
        cookies: &CookieJar<'_>,
        db: &mut DbPoolConnection,
    ) -> ResultResponse<Redirect> {
        let state = cookies
            .get(STATE_COOKIE_NAME)
            .map(|c| c.value())
            .ok_or_else(|| {
                error!(
                    "No state-type cookie found for {} callback",
                    Self::SERVICE_NAME
                );
                Status::BadRequest
            })?;

        cookies.remove(Cookie::from(STATE_COOKIE_NAME));

        let redirect = if state == LOGIN_STATE {
            self.handle_login_callback(db, cookies).await
        } else if state == LINK_STATE && user.is_some() {
            self.handle_link_callback(db, user.unwrap()).await
        } else {
            return Err(Status::BadRequest.into());
        }
        .with_context(|| format!("Error handling OAuth callback from {}", Self::SERVICE_NAME))??;
        Ok(redirect)
    }

    async fn handle_link_callback(
        &self,
        db: &mut DbPoolConnection,
        user: &User,
    ) -> Result<Result<Redirect, Status>> {
        let user_info = self.fetch_user_info().await?;
        self.link_to(db, user, user_info).await.map(|linked| {
            if linked {
                Ok(
                    Message::success(&format!("Linked your account to {}", Self::SERVICE_NAME))
                        .to("/settings/account"),
                )
            } else {
                Ok(Message::error(&format!(
                    "This {} account is already linked to another account",
                    Self::SERVICE_NAME
                ))
                .to("/settings/account"))
            }
        })
    }

    async fn handle_login_callback(
        &self,
        db: &mut DbPoolConnection,
        cookies: &CookieJar<'_>,
    ) -> Result<Result<Redirect, Status>> {
        let user_info = self.fetch_user_info().await?;

        let db_conn = &mut *db;

        let user = self
            .get_user(db_conn, user_info)
            .await
            .with_context(|| format!("Failed to get user info from {}", Self::SERVICE_NAME))?;

        let redirect = cookies
            .get(REDIRECT_COOKIE_NAME)
            .map(|c| c.value().to_string())
            .unwrap_or_else(|| "/".to_string());

        cookies.remove(Cookie::from(REDIRECT_COOKIE_NAME));

        if let Some(user) = user {
            user.login(db_conn, cookies)
                .await
                .with_context(|| format!("Failed to login user from {}", Self::SERVICE_NAME))?;
            Ok(Ok(Redirect::to(redirect)))
        } else {
            Ok(Ok(Message::error(&format!(
                "No account found for this {} account",
                Self::SERVICE_NAME
            ))
            .to_with_params(
                LOGIN_URI,
                vec![(REDIRECT_COOKIE_NAME, &redirect)],
            )))
        }
    }

    async fn handle_unlink(db: &mut DbPoolConnection, user: &User) -> ResultResponse<Redirect> {
        Self::unlink(db, user).await?;
        Ok(Message::success(&format!(
            "Unlinked your account from {}",
            Self::SERVICE_NAME
        ))
        .to("/settings/account"))
    }

    async fn fetch_user_info(&self) -> Result<Self::IntermediateUserInfo> {
        let resp = self
            .get_request_client()
            .send()
            .await
            .with_context(|| format!("Failed to send request to {}", Self::SERVICE_NAME))?;

        if resp.status().is_success() {
            let user_info = resp
                .json::<Self::IntermediateUserInfo>()
                .await
                .with_context(|| {
                    format!("Failed to parse user info from {}", Self::SERVICE_NAME)
                })?;
            Ok(user_info)
        } else {
            Err(anyhow!(
                "Failed to get user info from {}: {}",
                Self::SERVICE_NAME,
                resp.status()
            ))
        }
    }
}

mod prelude {
    pub use super::CallbackHandler;
    pub use rocket::{fairing::AdHoc, get, http::CookieJar, response::Redirect, routes};
    pub use rocket_oauth2::{OAuth2, TokenResponse};
    pub use sqlx::sqlite::SqliteQueryResult;

    pub use crate::{
        auth::users::User,
        db::{DbConnection, DbPoolConnection},
        error::prelude::*,
        oauth_fairing,
    };
}

#[macro_export]
macro_rules! oauth_fairing {
    ($name: literal, $route: ident, $handler: ident, $scopes: expr) => {
        #[get("/login")]
        fn login(oauth2: OAuth2<$handler>, cookies: &CookieJar<'_>) -> ResultResponse<Redirect> {
            $handler::put_login_cookie(cookies);
            let redirect = oauth2.get_redirect(cookies, &$scopes).context(concat!(
                "Error getting ",
                $name,
                " redirect"
            ))?;
            Ok(redirect)
        }

        #[get("/link")]
        fn link(
            oauth2: OAuth2<$handler>,
            _user: &User,
            cookies: &CookieJar<'_>,
        ) -> ResultResponse<Redirect> {
            $handler::put_link_cookie(cookies);
            let redirect = oauth2.get_redirect(cookies, &$scopes).context(concat!(
                "Error getting ",
                $name,
                " redirect"
            ))?;
            Ok(redirect)
        }

        #[get("/callback")]
        async fn callback(
            mut db: DbConnection,
            token: TokenResponse<$handler>,
            user: Option<&User>,
            cookies: &CookieJar<'_>,
        ) -> ResultResponse<Redirect> {
            let handler = $handler(token.access_token().to_string());
            handler.handle_callback(user, cookies, &mut db).await
        }

        #[get("/unlink")]
        async fn unlink(mut db: DbConnection, user: &User) -> ResultResponse<Redirect> {
            $handler::handle_unlink(&mut db, user).await
        }

        pub fn stage() -> AdHoc {
            AdHoc::on_ignite(concat!($name, " Auth"), |rocket| async {
                rocket
                    .attach(OAuth2::<$handler>::fairing(stringify!($route)))
                    .mount(
                        concat!("/auth/", stringify!($route)),
                        routes![login, callback, link, unlink,],
                    )
            })
        }
    };
}
