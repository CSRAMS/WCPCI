use rocket::{get, http::CookieJar, post, response::Redirect};
use rocket_dyn_templates::Template;

use crate::{
    auth::{
        csrf::{CsrfToken, VerifyCsrfToken},
        sessions::Session,
        users::User,
    },
    context_with_base_authed,
    db::DbConnection,
    error::prelude::*,
    messages::Message,
};

#[get("/account/delete")]
pub async fn delete_user_get(user: &User, _token: &CsrfToken) -> Template {
    let ctx = context_with_base_authed!(user,);
    Template::render("settings/delete", ctx)
}

#[post("/account/delete")]
pub async fn delete_user_post(
    mut db: DbConnection,
    user: &User,
    cookies: &CookieJar<'_>,
    _token: &VerifyCsrfToken,
) -> ResultResponse<Redirect> {
    user.delete(&mut db).await?;
    cookies.remove_private(Session::TOKEN_COOKIE_NAME);
    Ok(Message::info("Account deleted").to("/"))
}
