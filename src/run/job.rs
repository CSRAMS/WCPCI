use anyhow::anyhow;
use chrono::NaiveDateTime;
use log::{error, info};

use crate::{
    error::prelude::*,
    problems::TestCase,
    run::{runner::CaseError, worker::WorkerMessage},
};

use super::{languages::ComputedRunData, runner::Runner};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "status", content = "content", rename_all = "camelCase")]
pub enum CaseStatus {
    #[default]
    Pending,
    Running,
    // Passed, optional output
    Passed(Option<String>),
    NotRun,
    // Penalty, Error
    Failed(bool, String),
}

impl CaseStatus {
    pub fn to_name(&self) -> &str {
        match self {
            Self::Pending => "Pending",
            Self::Running => "Running",
            Self::Passed(_) => "Passed",
            Self::NotRun => "NotRun",
            Self::Failed(_, _) => "Failed",
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JobState {
    Judging {
        cases: Vec<CaseStatus>,
        complete: bool,
    },
    Testing {
        status: CaseStatus,
    },
}

impl JobState {
    pub fn new_judging(cases: usize) -> Self {
        Self::Judging {
            cases: vec![CaseStatus::Pending; cases],
            complete: false,
        }
    }

    pub fn new_testing() -> Self {
        Self::Testing {
            status: CaseStatus::Pending,
        }
    }

    pub fn new_for_op(op: &JobOperation) -> Self {
        match op {
            JobOperation::Judging(cases) => Self::new_judging(cases.len()),
            JobOperation::Testing(_) => Self::new_testing(),
        }
    }

    pub fn last_error(&self) -> (usize, bool, Option<String>) {
        match self {
            Self::Judging { cases, .. } => cases
                .iter()
                .enumerate()
                .find_map(|(i, c)| {
                    if let CaseStatus::Failed(penalty, e) = c {
                        Some((i, *penalty, Some(e.clone())))
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| (self.len(), false, None)),
            Self::Testing { status } => {
                if let CaseStatus::Failed(penalty, e) = status {
                    (0, *penalty, Some(e.clone()))
                } else {
                    (0, false, None)
                }
            }
        }
    }

    pub fn len(&self) -> usize {
        match self {
            Self::Judging { cases, .. } => cases.len(),
            Self::Testing { .. } => 1,
        }
    }

    pub fn complete(&self) -> bool {
        match self {
            Self::Judging { complete, .. } => *complete,
            Self::Testing { status } => matches!(
                status,
                CaseStatus::Passed(_) | CaseStatus::Failed(_, _) | CaseStatus::NotRun
            ),
        }
    }

    pub fn start_first(&mut self) {
        match self {
            Self::Judging { cases, .. } => {
                cases[0] = CaseStatus::Running;
            }
            Self::Testing { status } => {
                *status = CaseStatus::Running;
            }
        }
    }

    pub fn force_fail(&mut self, msg: &str) {
        let msg = msg.to_string();
        match self {
            Self::Judging { cases, .. } => {
                let idx = cases
                    .iter()
                    .position(|c| matches!(c, CaseStatus::Running))
                    .unwrap_or(0);
                self.complete_case(idx, CaseStatus::Failed(false, msg));
            }
            Self::Testing { status } => {
                *status = CaseStatus::Failed(true, msg);
            }
        }
    }

    pub fn complete_case(&mut self, idx: usize, status: CaseStatus) {
        match self {
            Self::Judging { cases, complete } => {
                if idx == cases.len() - 1 {
                    *complete = true;
                } else if matches!(&status, CaseStatus::Failed(_, _)) {
                    cases
                        .iter_mut()
                        .skip(idx + 1)
                        .for_each(|c| *c = CaseStatus::NotRun);
                    *complete = true;
                } else {
                    cases[idx + 1] = CaseStatus::Running;
                }
                cases[idx] = status;
            }
            Self::Testing { status: my_status } => {
                *my_status = status;
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum JobOperation {
    Judging(Vec<TestCase>),
    Testing(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobRequest {
    pub id: u64,
    pub user_id: i64,
    pub contest_id: i64,
    pub problem_id: i64,
    pub program: String,
    pub language_key: String,
    pub language: ComputedRunData,
    pub cpu_time: i64,
    pub op: JobOperation,
}

pub struct Job {
    pub id: u64,
    user_id: i64,
    runner: Runner,
    op: JobOperation,
    pub state: JobState,
    started_at: NaiveDateTime,
}

impl Job {
    pub fn new(request: JobRequest) -> Result<Self> {
        let mut state = match request.op {
            JobOperation::Judging(ref cases) => JobState::new_judging(cases.len()),
            JobOperation::Testing(_) => JobState::new_testing(),
        };

        let res = Runner::new(
            &request.language.compile_cmd,
            &request.language.run_cmd,
            &request.language.file_name,
            &request.program,
            request.cpu_time,
        );

        match res {
            Ok(runner) => {
                info!("Job {} Runner created", request.id);
                Ok(Self {
                    id: request.id,
                    runner,
                    state,
                    user_id: request.user_id,
                    op: request.op,
                    started_at: chrono::offset::Utc::now().naive_utc(),
                })
            }
            Err(e) => {
                state.complete_case(0, e.clone().into());
                Err(anyhow!(
                    "Job {} Couldn't create runner: {:?}",
                    request.id,
                    e
                ))
            }
        }
    }

    pub fn run(mut self) -> (JobState, NaiveDateTime) {
        self.state.start_first();
        self.publish_state();
        if let Err(why) = self.runner.compile() {
            info!("Job {} Compilation Failed: {:?}", self.id, why);
            if matches!(&self.state, JobState::Testing { .. }) {
                match why {
                    CaseError::Compilation(e) => {
                        self.state.complete_case(
                            0,
                            CaseStatus::Failed(false, format!("Compile Error: {}", e)),
                        );
                    }
                    _ => {
                        self.state.complete_case(0, why.into());
                    }
                }
            } else {
                self.state.complete_case(0, why.into());
            }
            self.publish_state();
            return (self.state, self.started_at);
        }
        info!(
            "Job {} Starting, requested by user {}",
            self.id, self.user_id
        );
        match &self.op {
            JobOperation::Judging(cases) => {
                for (i, case) in cases.iter().enumerate() {
                    info!("Job {} Running Case {}", self.id, i + 1);
                    let status = match self.runner.run_case(case) {
                        Ok(_) => CaseStatus::Passed(None),
                        Err(e) => match &e {
                            CaseError::Judge(ref why) => {
                                error!(
                                    "Job {} Case {} had a judging error: {:?}",
                                    self.id,
                                    i + 1,
                                    why
                                );
                                e.into()
                            }
                            _ => e.into(),
                        },
                    };
                    info!(
                        "Job {} Case {} finished with status {:?}",
                        self.id,
                        i + 1,
                        status.to_name()
                    );
                    self.state.complete_case(i, status);
                    self.publish_state();
                    if self.state.complete() {
                        break;
                    }
                }
            }
            JobOperation::Testing(input) => {
                info!("Job {} Running Test", self.id);
                let status = match self.runner.run_cmd(input) {
                    Ok(out) => CaseStatus::Passed(Some(out)),
                    Err(e) => match &e {
                        CaseError::Judge(ref why) => {
                            error!("Job {} Test had a judging error: {:?}", self.id, why);
                            e.into()
                        }
                        CaseError::Runtime(ref msg) => {
                            CaseStatus::Failed(true, format!("Runtime Error: {}", msg))
                        }
                        _ => e.into(),
                    },
                };
                info!(
                    "Job {} Test finished with status {:?}",
                    self.id,
                    status.to_name()
                );
                self.state.complete_case(0, status);
                self.publish_state();
            }
        }

        info!("Job {} Finished", self.id);
        (self.state, self.started_at)
    }

    pub fn publish_state(&self) {
        let msg = WorkerMessage::StateChange(self.state.clone());
        let state_str = serde_json::to_string(&msg).unwrap();
        println!("{}", state_str);
    }
}
