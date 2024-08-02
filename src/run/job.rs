use core::fmt;
use std::{
    fmt::{Display, Formatter},
    time::Instant,
};

use chrono::NaiveDateTime;

use crate::{
    error::prelude::*,
    problems::TestCase,
    run::worker::{DiagnosticInfo, Worker},
};

use super::{
    config::LanguageRunnerInfo, manager::ShutdownReceiver, worker::CaseError,
    worker::IsolationConfig, JobStateSender,
};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(tag = "status", content = "content", rename_all = "camelCase")]
pub enum CaseStatus {
    #[default]
    Pending,
    Running,
    // Passed, output
    Passed(String),
    NotRun,
    /// Penalty, Error
    Failed(bool, String),
}

impl CaseStatus {
    pub fn from_case_error(e: CaseError, details: bool) -> Self {
        let msg = e.to_string(details);
        Self::Failed(matches!(e, CaseError::Logic | CaseError::Runtime(_)), msg)
    }
}

impl Display for CaseStatus {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::Pending => write!(f, "[ ]"),
            Self::Running => write!(f, "[⧗]"),
            Self::Passed(_) => write!(f, "[🗸]"),
            Self::NotRun => write!(f, "[/]"),
            Self::Failed(_, _) => write!(f, "[𐄂]"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum JobState {
    Judging {
        cases: Vec<CaseStatus>,
        idx: usize,
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
            idx: 0,
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

    pub fn is_testing(&self) -> bool {
        matches!(self, Self::Testing { .. })
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
                // Shouldn't be empty, but to avoid a panic just in case
                if let Some(c) = cases.get_mut(0) {
                    *c = CaseStatus::Running;
                }
            }
            Self::Testing { status } => {
                *status = CaseStatus::Running;
            }
        }
    }

    pub fn complete_case(&mut self, status: CaseStatus) {
        match self {
            Self::Judging {
                cases,
                idx,
                complete,
            } => {
                if *idx == cases.len() - 1 {
                    *complete = true;
                } else if matches!(&status, CaseStatus::Failed(_, _)) {
                    cases
                        .iter_mut()
                        .skip(*idx + 1)
                        .for_each(|c| *c = CaseStatus::NotRun);
                    *complete = true;
                } else {
                    cases[*idx + 1] = CaseStatus::Running;
                }
                cases[*idx] = status;
                if !*complete {
                    *idx += 1;
                }
            }
            Self::Testing { status: my_status } => {
                *my_status = status;
            }
        }
    }
}

impl Display for JobState {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self {
            Self::Judging { cases, .. } => {
                for (i, c) in cases.iter().enumerate() {
                    if i != 0 {
                        write!(f, " ")?;
                    }
                    write!(f, "{}", c)?;
                }
                Ok(())
            }
            Self::Testing { status } => write!(f, "{}", status),
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
    pub problem_id: i64,
    pub contest_id: i64,
    pub program: String,
    pub language_key: String,
    pub language: LanguageRunnerInfo,
    pub cpu_time: i64,
    pub op: JobOperation,
}

struct JobContext {
    id: u64,
    state: JobState,
    sender: JobStateSender,
    ins: Instant,
}

fn publish_state(sender: &JobStateSender, state: JobState) {
    if let Err(why) = sender.send(state) {
        error!("Couldn't send state update: {:?}", why);
    }
}

impl JobContext {
    fn new(req: &JobRequest, sender: JobStateSender) -> Self {
        Self {
            id: req.id,
            ins: Instant::now(),
            state: JobState::new_for_op(&req.op),
            sender,
        }
    }

    pub fn publish_state(&mut self) {
        let elapsed = self.ins.elapsed();
        info!("Job {} State: {} ({:?})", self.id, self.state, elapsed);
        publish_state(&self.sender, self.state.clone());
        self.ins = Instant::now();
    }
}

pub async fn run_job(
    request: &JobRequest,
    state_tx: JobStateSender,
    shutdown_rx: ShutdownReceiver,
    isolation: &IsolationConfig,
) -> (JobState, NaiveDateTime) {
    let started_at = chrono::offset::Utc::now().naive_utc();
    let tx = state_tx.clone();
    let rx = state_tx.subscribe();
    let res = _run_job(
        state_tx,
        shutdown_rx,
        request,
        request.language.clone(),
        isolation.clone(),
    )
    .await;
    match res {
        Ok(state) => (state, started_at),
        Err(e) => {
            if let CaseError::Judge(ref e) = e {
                error!("Job {} Judge Error: {}", request.id, e);
            }
            let mut last_state = rx.borrow().clone();
            let details = last_state.is_testing();
            last_state.complete_case(CaseStatus::from_case_error(e, details));
            info!("Job {} State: {}", request.id, last_state);
            publish_state(&tx, last_state.clone());
            (last_state, started_at)
        }
    }
}

async fn _run_job(
    state_tx: JobStateSender,
    shutdown_rx: ShutdownReceiver,
    request: &JobRequest,
    language: LanguageRunnerInfo,
    isolation: IsolationConfig,
) -> Result<JobState, CaseError> {
    let mut ctx = JobContext::new(request, state_tx);

    ctx.state.start_first();
    ctx.publish_state();

    let diag = DiagnosticInfo {
        run_id: request.id,
        user_id: request.user_id,
        problem_id: request.problem_id,
        lang: request.language_key.clone(),
    };

    let mut worker = Worker::new(request.id, request, shutdown_rx, language, isolation, diag)
        .await
        .context("Worker Creation Failed")?;

    let res = run_worker(&mut worker, request, &mut ctx).await;

    worker.finish().await?;

    res.map(|_| ctx.state)
}

async fn run_worker(
    worker: &mut Worker,
    request: &JobRequest,
    ctx: &mut JobContext,
) -> Result<(), CaseError> {
    worker.compile().await?;
    match &request.op {
        JobOperation::Testing(stdin) => {
            let output = worker.run_cmd(Some(stdin)).await?;
            ctx.state.complete_case(CaseStatus::Passed(output));
            ctx.publish_state();
        }
        JobOperation::Judging(cases) => {
            for case in cases.iter() {
                let output = worker.run_case(case).await?;
                ctx.state.complete_case(CaseStatus::Passed(output));
                ctx.publish_state();
                if ctx.state.complete() {
                    break;
                }
            }
        }
    }
    Ok(())
}
