use std::collections::HashMap;
use std::sync::Arc;

use chrono::NaiveDateTime;
use log::error;
use rocket_db_pools::Pool;
use tokio::sync::Mutex;

use crate::contests::{Contest, Participant};
use crate::db::{DbPool, DbPoolConnection};
use crate::error::prelude::*;
use crate::leaderboard::LeaderboardManagerHandle;
use crate::problems::{JudgeRun, ProblemCompletion, TestCase};

use super::job::JobRequest;

use super::languages::{ComputedRunData, RunConfig};
use super::{JobState, JobStateReceiver, Worker};

type UserId = i64;

type RunHandle = Arc<Mutex<Option<(i64, JobStateReceiver, (ShutDownSender, ShutdownReceiver))>>>;

pub type JobStartedMessage = (UserId, i64, JobStateReceiver);
pub type JobStartedReceiver = tokio::sync::broadcast::Receiver<JobStartedMessage>;
pub type JobStartedSender = tokio::sync::broadcast::Sender<JobStartedMessage>;

pub type ProblemUpdatedMessage = ();
pub type ProblemUpdatedReceiver = tokio::sync::watch::Receiver<ProblemUpdatedMessage>;
pub type ProblemUpdatedSender = tokio::sync::watch::Sender<ProblemUpdatedMessage>;

pub type ShutdownReceiver = tokio::sync::watch::Receiver<bool>;
pub type ShutDownSender = tokio::sync::watch::Sender<bool>;

pub struct RunManager {
    config: RunConfig,
    language_run_data: HashMap<String, ComputedRunData>,
    id_counter: u64,
    jobs: HashMap<UserId, RunHandle>,
    db_pool: DbPool,
    job_started_channel: (JobStartedSender, JobStartedReceiver),
    problem_updated_channels: HashMap<i64, ProblemUpdatedSender>,
    leaderboard_handle: LeaderboardManagerHandle,
    shutdown_rx: ShutdownReceiver,
}

impl RunManager {
    pub fn new(
        config: RunConfig,
        leaderboard_manager: LeaderboardManagerHandle,
        pool: DbPool,
        shutdown_rx: ShutdownReceiver,
    ) -> Self {
        let (tx, rx) = tokio::sync::broadcast::channel(10);
        let run_data = config
            .languages
            .iter()
            .map(|(k, l)| (k.clone(), ComputedRunData::compute(&config, &l.runner)))
            .collect();

        Self {
            config,
            language_run_data: run_data,
            id_counter: 1,
            leaderboard_handle: leaderboard_manager,
            jobs: HashMap::with_capacity(10),
            db_pool: pool,
            job_started_channel: (tx, rx),
            problem_updated_channels: HashMap::with_capacity(5),
            shutdown_rx,
        }
    }

    pub async fn all_active_jobs(&self) -> Vec<(UserId, i64)> {
        let mut active_jobs = Vec::with_capacity(self.jobs.len());
        for (user_id, handle) in self.jobs.iter() {
            let handle = handle.lock().await;
            if let Some((problem_id, _, _)) = handle.as_ref() {
                active_jobs.push((*user_id, *problem_id));
            }
        }
        active_jobs
    }

    pub fn subscribe(&self) -> JobStartedReceiver {
        self.job_started_channel.0.subscribe()
    }

    pub async fn subscribe_shutdown(&self, user_id: &UserId) -> ShutdownReceiver {
        if let Some(handle) = self.jobs.get(user_id) {
            let handle = handle.lock().await;
            if let Some((_, _, (_, rx))) = handle.as_ref() {
                rx.clone()
            } else {
                self.shutdown_rx.clone()
            }
        } else {
            self.shutdown_rx.clone()
        }
    }

    pub fn get_request_id(&mut self) -> u64 {
        let id = self.id_counter;
        self.id_counter += 1;
        id
    }

    pub fn get_language_config(&self, language_key: &str) -> Option<ComputedRunData> {
        self.language_run_data.get(language_key).cloned()
    }

    async fn start_job(&mut self, request: JobRequest, cases: Vec<TestCase>) -> Result<(), String> {
        if request.program.len() > self.config.max_program_length {
            return Err(format!(
                "Program too long, max length is {} bytes",
                self.config.max_program_length
            ));
        }

        let user_id = request.user_id;
        let problem_id = request.problem_id;
        let contest_id = request.contest_id;
        let program = request.program.clone();

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let (state_tx, state_rx) = tokio::sync::watch::channel(JobState::new_for_op(&request.op));

        let shutdown_rx_handle = shutdown_rx.clone();

        let handle = Arc::new(Mutex::new(Some((
            problem_id,
            state_rx.clone(),
            (shutdown_tx, shutdown_rx_handle),
        ))));

        self.jobs.insert(user_id, handle.clone());

        let pool = self.db_pool.clone();

        let leaderboard_handle = self.leaderboard_handle.clone();

        let shutdown_rx_worker = shutdown_rx.clone();

        let worker = Worker::new(request.id, request.clone(), cases, state_tx)
            .await
            .map_err(|e| {
                error!("Couldn't create worker: {:?}", e);
                "JudgeError: Couldn't create worker".to_string()
            })?;

        tokio::spawn(async move {
            let (state, ran_at) = worker.spawn(shutdown_rx_worker).await;

            handle.lock().await.take();
            drop(handle);
            if !matches!(state, JobState::Judging { .. }) {
                return;
            }
            match pool.get().await {
                Ok(mut conn) => {
                    let run = JudgeRun::from_job_state(
                        problem_id,
                        user_id,
                        program,
                        request.language_key.clone(),
                        &state,
                        ran_at,
                    );
                    if let Err(why) = Self::save_run(
                        &mut conn,
                        contest_id,
                        problem_id,
                        user_id,
                        run,
                        ran_at,
                        state.last_error().1,
                        leaderboard_handle,
                    )
                    .await
                    {
                        error!("Couldn't save run: {:?}", why);
                    }
                }
                Err(e) => {
                    error!("Couldn't get db connection: {:?}", e);
                }
            }
        });

        self.job_started_channel
            .0
            .send((user_id, problem_id, state_rx))
            .ok();

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn save_run(
        conn: &mut DbPoolConnection,
        contest_id: i64,
        problem_id: i64,
        user_id: i64,
        judge_run: JudgeRun,
        ran_at: NaiveDateTime,
        penalty_applies: bool,
        leaderboard_handle: LeaderboardManagerHandle,
    ) -> Result {
        let contest = Contest::get(conn, contest_id)
            .await?
            .ok_or_else(|| anyhow!("Couldn't find contest with id {}", contest_id))?;

        let success = judge_run.success();
        judge_run.write_to_db(conn).await?;

        let participant = Participant::get(conn, contest_id, user_id).await?;

        if participant.as_ref().map_or(true, |p| p.is_judge) || !contest.is_running() {
            return Ok(());
        }

        let participant = participant.unwrap();

        let mut completion =
            ProblemCompletion::get_for_problem_and_participant(conn, problem_id, participant.p_id)
                .await
                .context("While getting problem completion")?
                .unwrap_or_else(|| {
                    ProblemCompletion::temp(
                        participant.p_id,
                        problem_id,
                        Some(ran_at).filter(|_| success),
                    )
                });

        if success && completion.completed_at.is_none() {
            completion.completed_at = Some(ran_at);
        } else if penalty_applies && completion.completed_at.is_none() {
            completion.number_wrong += 1;
        }

        completion.upsert(conn).await?;

        if completion.completed_at.is_some() {
            let mut leaderboard_manager = leaderboard_handle.lock().await;
            leaderboard_manager
                .process_completion(&completion, &contest)
                .await;
        }

        Ok(())
    }

    pub fn get_handle_for_problem(&mut self, problem_id: i64) -> ProblemUpdatedReceiver {
        if let Some(handle) = self.problem_updated_channels.get(&problem_id) {
            handle.subscribe()
        } else {
            let (tx, rx) = tokio::sync::watch::channel(());
            self.problem_updated_channels.insert(problem_id, tx);
            rx
        }
    }

    pub async fn shutdown_job(&mut self, user_id: UserId) {
        if let Some(handle) = self.jobs.remove(&user_id) {
            let handle = handle.lock().await;
            if let Some((_, _, (tx, _))) = handle.as_ref() {
                tx.send(true).ok();
            }
        }
    }

    pub async fn shutdown(&mut self) {
        for (_, handle) in self.jobs.drain() {
            let handle = handle.lock().await;
            if let Some((_, _, (tx, _))) = handle.as_ref() {
                tx.send(true).ok();
            }
        }
    }

    pub async fn update_problem(&mut self, problem_id: i64) {
        if let Some(handle) = self.problem_updated_channels.remove(&problem_id) {
            handle.send(()).ok();
        }
    }

    pub async fn get_handle(&self, user_id: UserId, problem_id: i64) -> Option<JobStateReceiver> {
        if let Some(handle) = self.jobs.get(&user_id) {
            let handle = handle.lock().await;
            handle
                .as_ref()
                .filter(|(id, _, _)| *id == problem_id)
                .map(|(_, rx, _)| rx.clone())
        } else {
            None
        }
    }

    pub async fn request_job(
        &mut self,
        request: JobRequest,
        cases: Vec<TestCase>,
    ) -> Result<(), String> {
        if let Some(handle) = self.jobs.get(&request.user_id) {
            let handle = handle.lock().await;
            if handle.is_some() {
                Err("User already has a job running".to_string())
            } else {
                drop(handle);
                self.start_job(request, cases).await
            }
        } else {
            self.start_job(request, cases).await
        }
    }
}
