use log::error;
use rocket::{get, http::Status, post, response::Redirect, State};
use rocket_dyn_templates::Template;

use crate::{
    auth::{
        csrf::{CsrfToken, VerifyCsrfToken},
        users::{Admin, User},
    },
    context_with_base_authed,
    db::DbConnection,
    error::prelude::*,
    leaderboard::LeaderboardManagerHandle,
    messages::Message,
};

#[get("/users")]
pub async fn users(mut db: DbConnection, user: &User, _admin: &Admin) -> ResultResponse<Template> {
    let users = User::list(&mut db).await?;
    let ctx = context_with_base_authed!(user, users);
    Ok(Template::render("admin/users", ctx))
}

#[get("/users/<id>/delete")]
pub async fn delete_user_get(
    id: i64,
    mut db: DbConnection,
    user: &User,
    _admin: &Admin,
    _token: &CsrfToken,
) -> ResultResponse<Template> {
    let target_user = User::get_or_404(&mut db, id).await?;
    let ctx = context_with_base_authed!(user, target_user);
    Ok(Template::render("admin/delete_user", ctx))
}

#[post("/users/<id>/delete")]
pub async fn delete_user_post(
    id: i64,
    mut db: DbConnection,
    leaderboards: &State<LeaderboardManagerHandle>,
    _admin: &Admin,
    _token: &VerifyCsrfToken,
) -> ResultResponse<Redirect> {
    let target_user = User::get_or_404(&mut db, id).await?;
    target_user.delete(&mut db).await.map_err(|e| {
        error!("Failed to delete user: {:?}", e);
        Status::InternalServerError
    })?;
    let mut leaderboard_manager = leaderboards.lock().await;
    leaderboard_manager.delete_user(id).await;
    Ok(Message::success("User deleted").to("/admin/users"))
}
