use rocket::{fairing::AdHoc, get, routes};
use rocket_dyn_templates::Template;

use crate::{
    auth::users::{Admin, User},
    context_with_base_authed,
    db::DbConnection,
    error::prelude::*,
};

use super::Contest;

mod completions;
mod participants;
mod runs;

#[get("/contests/<contest_id>/admin")]
async fn contest_admin(
    mut db: DbConnection,
    contest_id: i64,
    user: &User,
    admin: Option<&Admin>,
) -> ResultResponse<Template> {
    let (contest, _) =
        Contest::get_or_404_assert_can_edit(&mut db, contest_id, user, admin).await?;
    let ctx = context_with_base_authed!(user, contest);
    Ok(Template::render("contests/admin", ctx))
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("Contest Admin", |rocket| async {
        rocket.mount(
            "/",
            routes![
                contest_admin,
                participants::participants,
                participants::kick_participant_get,
                participants::kick_participant_post,
                runs::runs,
                runs::cancel,
                runs::cancel_post,
                runs::problem,
                runs::view_user_run,
                completions::edit_completion,
                completions::edit_completion_post,
            ],
        )
    })
}
