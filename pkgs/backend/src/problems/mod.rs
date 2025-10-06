#![allow(clippy::blocks_in_conditions)] // Needed for the derive of FromForm, rocket is weird

use std::collections::HashMap;

use rocket::{fairing::AdHoc, http::Status, routes, FromForm};

mod cases;
mod completions;
mod delete;
mod edit;
mod io;
mod new;
mod runs;
mod view;

pub use cases::TestCase;
pub use completions::ProblemCompletion;
pub use runs::JudgeRun;

use crate::{db::DbPoolConnection, error::prelude::*, template::TemplatedForm, ResultResponse};

use self::cases::TestCaseForm;

#[derive(Serialize)]
pub struct Problem {
    pub id: i64,
    pub contest_id: i64,
    pub name: String,
    pub slug: String,
    pub description: String,
    pub cpu_time: i64,
    pub memory_limit: i64,
}

impl Problem {
    pub async fn by_id(
        db: &mut DbPoolConnection,
        contest_id: i64,
        id: i64,
    ) -> Result<Option<Self>> {
        sqlx::query_as!(
            Problem,
            "SELECT * FROM problem WHERE id = ? AND contest_id = ?",
            id,
            contest_id
        )
        .fetch_optional(&mut **db)
        .await
        .with_context(|| format!("Failed to get problem with id {}", id))
    }

    pub async fn slug_exists(
        db: &mut DbPoolConnection,
        slug: &str,
        contest_id: i64,
        problem_id: Option<i64>,
    ) -> Result<bool> {
        if let Some(problem_id) = problem_id {
            sqlx::query!(
                "SELECT * FROM problem WHERE contest_id = ? AND id != ? AND slug = ?",
                contest_id,
                problem_id,
                slug
            )
            .fetch_optional(&mut **db)
            .await
            .map(|o| o.is_some())
            .context("Failed to check if slug exists")
        } else {
            sqlx::query!(
                "SELECT * FROM problem WHERE contest_id = ? AND slug = ?",
                contest_id,
                slug
            )
            .fetch_optional(&mut **db)
            .await
            .map(|o| o.is_some())
            .context("Failed to check if slug exists")
        }
    }

    pub async fn get(
        db: &mut DbPoolConnection,
        contest_id: i64,
        slug: &str,
    ) -> Result<Option<Self>> {
        let problem = sqlx::query_as!(
            Problem,
            "SELECT * FROM problem WHERE contest_id = ? AND slug = ?",
            contest_id,
            slug
        )
        .fetch_optional(&mut **db)
        .await?;
        Ok(problem)
    }

    pub async fn get_or_404(
        db: &mut DbPoolConnection,
        contest_id: i64,
        slug: &str,
    ) -> ResultResponse<Self> {
        Self::get(db, contest_id, slug)
            .await?
            .ok_or(Status::NotFound.into())
    }

    pub async fn list(db: &mut DbPoolConnection, contest_id: i64) -> Result<Vec<Self>> {
        sqlx::query_as!(
            Problem,
            "SELECT * FROM problem WHERE contest_id = ?",
            contest_id
        )
        .fetch_all(&mut **db)
        .await
        .context("Failed to get all problems")
    }

    pub async fn insert(&self, db: &mut DbPoolConnection) -> Result<Problem> {
        sqlx::query_as!(
            Problem,
            "INSERT INTO problem (name, contest_id, slug, description, cpu_time, memory_limit) VALUES (?, ?, ?, ?, ?, ?) RETURNING *",
            self.name,
            self.contest_id,
            self.slug,
            self.description,
            self.cpu_time,
            self.memory_limit
        )
        .fetch_one(&mut **db)
        .await.context("Failed to insert new problem")
    }

    pub async fn update(&self, db: &mut DbPoolConnection) -> Result {
        sqlx::query_as!(
            Problem,
            "UPDATE problem SET name = ?, slug = ?, description = ?, cpu_time = ?, memory_limit = ? WHERE id = ?",
            self.name,
            self.slug,
            self.description,
            self.cpu_time,
            self.memory_limit,
            self.id,
        )
        .execute(&mut **db)
        .await
        .map(|_| ())
        .with_context(|| format!("Failed to update problem with id {}", self.id))
    }

    pub async fn delete(self, db: &mut DbPoolConnection) -> Result {
        sqlx::query!(
            "DELETE FROM problem WHERE id = ? AND contest_id = ?",
            self.id,
            self.contest_id
        )
        .execute(&mut **db)
        .await
        .map(|_| ())
        .with_context(|| format!("Failed to delete problem with id {}", self.id))
    }

    pub fn temp(contest_id: i64, form: &ProblemForm) -> Self {
        let slug = slug::slugify(form.name);
        Self {
            id: 0,
            contest_id,
            name: form.name.to_string(),
            slug,
            description: form.description.to_string(),
            cpu_time: form.cpu_time,
            memory_limit: form.memory_limit,
        }
    }
}

#[derive(FromForm)]
pub struct ProblemForm<'r> {
    #[field(validate = len(1..=32))]
    name: &'r str,
    description: &'r str,
    #[field(validate = range(1..=100))]
    cpu_time: i64,
    #[field(validate = range(1..))]
    memory_limit: i64,
    test_cases: Vec<TestCaseForm<'r>>,
}

pub struct ProblemFormTemplate<'r> {
    problem: Option<&'r Problem>,
    test_cases: Vec<TestCaseForm<'r>>,
}

impl<'r> TemplatedForm for ProblemFormTemplate<'r> {
    fn get_defaults(&mut self) -> HashMap<String, String> {
        if let Some(problem) = self.problem {
            let mut map = HashMap::from_iter([
                ("name".to_string(), problem.name.clone()),
                ("description".to_string(), problem.description.clone()),
                ("cpu_time".to_string(), problem.cpu_time.to_string()),
                ("memory_limit".to_string(), problem.memory_limit.to_string()),
            ]);
            for (i, case) in self.test_cases.iter().enumerate() {
                map.insert(format!("test_cases[{}].stdin", i), case.stdin.to_string());
                map.insert(
                    format!("test_cases[{}].expected_pattern", i),
                    case.expected_pattern.to_string(),
                );
                map.insert(
                    format!("test_cases[{}].use_regex", i),
                    case.use_regex.to_string(),
                );
                map.insert(
                    format!("test_cases[{}].case_insensitive", i),
                    case.case_insensitive.to_string(),
                );
            }
            map
        } else {
            HashMap::from_iter([
                ("name".to_string(), "".to_string()),
                ("description".to_string(), "".to_string()),
                ("cpu_time".to_string(), "1".to_string()),
                ("memory_limit".to_string(), "125".to_string()),
            ])
        }
    }
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("Problem Stage", |rocket| async {
        rocket.attach(io::stage()).mount(
            "/contests",
            routes![
                view::list_problems_get,
                view::view_problem_get,
                new::new_problem_get,
                new::new_problem_post,
                edit::edit_problem_get,
                edit::edit_problem_post,
                delete::delete_problem_get,
                delete::delete_problem_post,
                runs::runs
            ],
        )
    })
}
