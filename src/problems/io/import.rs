use std::collections::HashMap;

use rocket::{
    form::{Contextual, Error, Form},
    get, post, FromForm,
};
use rocket_dyn_templates::Template;

use crate::{
    auth::{
        csrf::CsrfToken,
        users::{Admin, User},
    },
    contests::Contest,
    context_with_base_authed,
    db::DbConnection,
    error::prelude::*,
    problems::{cases::TestCaseForm, Problem, ProblemForm, ProblemFormTemplate},
    template::{FormTemplateObject, TemplatedForm},
};

use super::ProblemData;

#[derive(FromForm, Clone)]
pub struct ProblemImportForm {
    data: String,
}

impl TemplatedForm for ProblemImportForm {
    fn get_defaults(&mut self) -> HashMap<String, String> {
        HashMap::from_iter(vec![("data".to_string(), "".to_string())])
    }
}

#[get("/contests/<contest_id>/problems/import")]
pub async fn problem_import(
    mut db: DbConnection,
    contest_id: i64,
    admin: Option<&Admin>,
    user: &User,
) -> ResultResponse<Template> {
    let contest = Contest::get_or_404_assert_can_edit(&mut db, contest_id, user, admin).await?;
    let form = ProblemImportForm {
        data: String::new(),
    };
    let form = FormTemplateObject::get(form);
    let ctx = context_with_base_authed!(user, contest, form);
    Ok(Template::render("problems/import", ctx))
}

#[post("/contests/<contest_id>/problems/import", data = "<form>")]
pub async fn problem_import_post(
    mut db: DbConnection,
    contest_id: i64,
    admin: Option<&Admin>,
    user: &User,
    _token: &CsrfToken,
    mut form: Form<Contextual<'_, ProblemImportForm>>,
) -> ResultResponse<Template> {
    let contest = Contest::get_or_404_assert_can_edit(&mut db, contest_id, user, admin).await?;
    if let Some(value) = form.value.clone() {
        match serde_json::from_str::<ProblemData>(value.data.as_str()) {
            Ok(problem_data) => {
                let problem_form = ProblemForm {
                    name: &problem_data.name,
                    description: &problem_data.description,
                    cpu_time: problem_data.cpu_time,
                    memory_limit: problem_data.memory_limit,
                    test_cases: problem_data
                        .cases
                        .iter()
                        .map(|c| TestCaseForm {
                            stdin: &c.stdin,
                            expected_pattern: &c.expected_pattern,
                            use_regex: c.use_regex,
                            case_insensitive: c.case_insensitive,
                        })
                        .collect(),
                };
                let problem = Problem::temp(contest_id, &problem_form);
                let cases = problem_data
                    .cases
                    .iter()
                    .map(|c| TestCaseForm {
                        stdin: &c.stdin,
                        expected_pattern: &c.expected_pattern,
                        use_regex: c.use_regex,
                        case_insensitive: c.case_insensitive,
                    })
                    .collect();
                let form_template = ProblemFormTemplate {
                    problem: Some(&problem),
                    test_cases: cases,
                };
                let form_template = FormTemplateObject::get(form_template);
                let ctx = context_with_base_authed!(user, contest, form: form_template);
                return Ok(Template::render("problems/import-2", ctx));
            }
            Err(e) => {
                let error =
                    Error::validation(format!("Invalid JSON passed: {}", e)).with_name("data");
                form.context.push_error(error);
            }
        }
    }
    let form_template = ProblemImportForm {
        data: String::new(),
    };
    let form_template = FormTemplateObject::from_rocket_context(form_template, &form.context);
    let ctx = context_with_base_authed!(user, contest, form: form_template);
    Ok(Template::render("problems/import", ctx))
}
