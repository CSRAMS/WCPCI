use rocket::{get, post};
use rocket_dyn_templates::Template;

use crate::{
    auth::{
        csrf::{CsrfToken, VerifyCsrfToken},
        users::{Admin, User},
    },
    context_with_base_authed,
    db::DbConnection,
    messages::Message,
    FormResponse, ResultResponse,
};

use super::Contest;

#[get("/<contest_id>/delete")]
pub async fn delete_contest_get(
    contest_id: i64,
    mut db: DbConnection,
    _token: &CsrfToken,
    user: &User,
    _admin: &Admin,
) -> ResultResponse<Template> {
    let contest = Contest::get_or_404(&mut db, contest_id).await?;
    let ctx = context_with_base_authed!(user, contest);
    Ok(Template::render("contests/delete", ctx))
}

#[post("/<contest_id>/delete")]
pub async fn delete_contest_post(
    contest_id: i64,
    mut db: DbConnection,
    _token: &VerifyCsrfToken,
    _admin: &Admin,
) -> FormResponse {
    let contest = Contest::get_or_404(&mut db, contest_id).await?;
    contest.delete(&mut db).await?;
    Ok(Message::success("Contest deleted").to("/contests"))
}
