use rocket::{fairing::AdHoc, routes};
use serde::Deserialize;

use crate::{db::DbPoolConnection, error::prelude::*};

use super::{Problem, TestCase};

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CaseData {
    stdin: String,
    expected_pattern: String,
    use_regex: bool,
    case_insensitive: bool,
}

impl From<TestCase> for CaseData {
    fn from(tc: TestCase) -> Self {
        Self {
            stdin: tc.stdin,
            expected_pattern: tc.expected_pattern,
            use_regex: tc.use_regex,
            case_insensitive: tc.case_insensitive,
        }
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProblemData {
    name: String,
    description: String,
    cpu_time: i64,
    memory_limit: i64,
    cases: Vec<CaseData>,
}

impl ProblemData {
    pub async fn get_for_problem(db: &mut DbPoolConnection, problem: &Problem) -> Result<Self> {
        let cases = TestCase::get_for_problem(db, problem.id)
            .await
            .context("Couldn't get cases")?;
        Ok(Self {
            name: problem.name.clone(),
            description: problem.description.clone(),
            cpu_time: problem.cpu_time,
            memory_limit: problem.memory_limit,
            cases: cases.into_iter().map(CaseData::from).collect(),
        })
    }
}

mod export;
mod import;

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("Problem Import Export", |rocket| async {
        rocket.mount(
            "/",
            routes![
                export::problem_export,
                import::problem_import,
                import::problem_import_post
            ],
        )
    })
}
