use rocket::{get, serde::json::Json};

use crate::{
    auth::users::{Admin, User},
    contests::Contest,
    db::DbConnection,
    error::prelude::*,
    problems::Problem,
};

use super::ProblemData;

#[get("/contests/<contest_id>/problems/<problem_slug>/export")]
pub async fn problem_export(
    mut db: DbConnection,
    contest_id: i64,
    admin: Option<&Admin>,
    user: &User,
    problem_slug: &str,
) -> ResultResponse<Json<ProblemData>> {
    Contest::get_or_404_assert_can_edit(&mut db, contest_id, user, admin).await?;
    let problem = Problem::get_or_404(&mut db, contest_id, problem_slug).await?;
    let data = ProblemData::get_for_problem(&mut db, &problem)
        .await
        .with_context(|| {
            format!("Couldn't export problem {problem_slug} from contest {contest_id}")
        })?;
    Ok(Json(data))
}
