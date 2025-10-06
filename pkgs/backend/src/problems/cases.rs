use rocket::FromForm;
use sqlx::prelude::FromRow;

use crate::{db::DbPoolConnection, error::prelude::*};

#[derive(Serialize, Deserialize, FromRow, Clone, Debug)]
pub struct TestCase {
    pub id: i64,
    problem_id: i64,
    ord: i64,
    pub stdin: String,
    pub expected_pattern: String,
    pub use_regex: bool,
    pub case_insensitive: bool,
}

impl TestCase {
    pub fn temp(form: &TestCaseForm, problem_id: i64, ord: i64) -> Self {
        Self {
            id: 0,
            problem_id,
            ord,
            stdin: form.stdin.to_string(),
            expected_pattern: form.expected_pattern.to_string(),
            use_regex: form.use_regex,
            case_insensitive: form.case_insensitive,
        }
    }

    pub fn from_vec(problem_id: i64, cases: &[TestCaseForm]) -> Vec<Self> {
        cases
            .iter()
            .enumerate()
            .map(|(i, c)| Self::temp(c, problem_id, i as i64))
            .collect()
    }

    pub async fn save_for_problem(
        db: &mut DbPoolConnection,
        problem_id: i64,
        cases: Vec<Self>,
    ) -> Result<Vec<Self>> {
        sqlx::query("DELETE FROM test_case WHERE problem_id = ? AND ord >= ?")
            .bind(problem_id)
            .bind(cases.len() as i64)
            .execute(&mut **db)
            .await
            .context("Failed to delete old test cases")?;
        let values_str = cases
            .iter()
            .map(|_| "(?, ?, ?, ?, ?, ?)")
            .collect::<Vec<_>>()
            .join(",");
        let query_str = format!("INSERT OR REPLACE INTO test_case (problem_id, ord, stdin, expected_pattern, use_regex, case_insensitive) VALUES {} RETURNING *", values_str);
        let mut query = sqlx::query(&query_str);
        for c in cases.iter() {
            query = query
                .bind(c.problem_id)
                .bind(c.ord)
                .bind(&c.stdin)
                .bind(&c.expected_pattern)
                .bind(c.use_regex)
                .bind(c.case_insensitive);
        }
        let res = query.fetch_all(&mut **db).await;
        res.context("Failed to upsert new test cases for problem")
            .and_then(|rows| {
                rows.into_iter()
                    .enumerate()
                    .map(|(i, row)| {
                        TestCase::from_row(&row)
                            .with_context(|| format!("Failed to parse row {}", i))
                    })
                    .collect()
            })
    }

    pub async fn get_for_problem(db: &mut DbPoolConnection, problem_id: i64) -> Result<Vec<Self>> {
        sqlx::query_as!(
            TestCase,
            "SELECT * FROM test_case WHERE problem_id = ? ORDER BY ord",
            problem_id
        )
        .fetch_all(&mut **db)
        .await
        .with_context(|| format!("Failed to get test cases for problem {}", problem_id))
    }

    pub async fn count_for_problem(db: &mut DbPoolConnection, problem_id: i64) -> Result<i64> {
        sqlx::query!("SELECT id FROM test_case WHERE problem_id = ?", problem_id)
            .fetch_all(&mut **db)
            .await
            .map(|rows| rows.len() as i64)
            .with_context(|| format!("Failed to count test cases for problem {}", problem_id))
    }

    pub fn to_form(&self) -> TestCaseForm<'_> {
        TestCaseForm {
            stdin: &self.stdin,
            expected_pattern: &self.expected_pattern,
            use_regex: self.use_regex,
            case_insensitive: self.case_insensitive,
        }
    }

    pub fn check_output(&self, output: &str) -> Result<bool, String> {
        if self.use_regex {
            let mut builder = regex::RegexBuilder::new(&self.expected_pattern);
            builder.case_insensitive(self.case_insensitive);
            let re = builder
                .build()
                .map_err(|e| format!("Couldn't build regex: {e:?}"))?;
            Ok(re.is_match(output.trim()))
        } else {
            let (output, expected) = (output.trim(), self.expected_pattern.trim());
            if self.case_insensitive {
                Ok(output.to_lowercase() == expected.to_lowercase())
            } else {
                Ok(output == expected)
            }
        }
    }
}

fn check_regex(pattern: &'_ str, enabled: bool) -> Result<(), rocket::form::Errors<'_>> {
    if enabled {
        regex::Regex::new(pattern)
            .map(|_| ())
            .map_err(|e| rocket::form::Error::custom(e).into())
    } else {
        Ok(())
    }
}

#[derive(Debug, FromForm, Serialize)]
pub struct TestCaseForm<'r> {
    #[field(validate = len(1..))]
    pub stdin: &'r str,
    #[field(validate = len(1..))]
    #[field(validate = check_regex(self.use_regex))]
    pub expected_pattern: &'r str,
    pub use_regex: bool,
    pub case_insensitive: bool,
}
