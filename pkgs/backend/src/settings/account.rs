use rocket::get;
use rocket_dyn_templates::Template;

use crate::{auth::users::User, context_with_base_authed};

#[get("/account")]
pub fn account_get(user: &User) -> Template {
    let ctx = context_with_base_authed!(user,);
    Template::render("settings/account", ctx)
}
