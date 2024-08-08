use std::collections::HashMap;
use std::sync::Arc;

use chrono::NaiveDateTime;
use log::error;
use rocket::figment::Profile;
use rocket_db_pools::Pool;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use crate::contests::{Contest, Participant};
use crate::db::{DbPool, DbPoolConnection};
use crate::error::prelude::*;
use crate::leaderboard::LeaderboardManagerHandle;
use crate::problems::{JudgeRun, ProblemCompletion};

use super::job::{run_job, JobOperation, JobRequest};
use super::worker::IsolationConfig;

use super::config::{LanguageRunnerInfo, RunConfig};
use super::{JobState, JobStateReceiver};

type UserId = i64;

type RunHandle = Arc<Mutex<Option<(i64, JobStateReceiver, CancellationToken)>>>;

pub type JobStartedMessage = (UserId, i64, JobStateReceiver);
pub type JobStartedReceiver = tokio::sync::broadcast::Receiver<JobStartedMessage>;
pub type JobStartedSender = tokio::sync::broadcast::Sender<JobStartedMessage>;

pub type ProblemUpdatedMessage = ();
pub type ProblemUpdatedReceiver = tokio::sync::watch::Receiver<ProblemUpdatedMessage>;
pub type ProblemUpdatedSender = tokio::sync::watch::Sender<ProblemUpdatedMessage>;

pub struct RunManager {
    config: RunConfig,
    isolation_config: IsolationConfig,
    language_runner_info: HashMap<String, LanguageRunnerInfo>,
    id_counter: u64,
    jobs: HashMap<UserId, RunHandle>,
    db_pool: DbPool,
    job_started_channel: (JobStartedSender, JobStartedReceiver),
    problem_updated_channels: HashMap<i64, ProblemUpdatedSender>,
    leaderboard_handle: LeaderboardManagerHandle,
    shutdown: CancellationToken,
}

pub struct ManagerJobRequest {
    pub user_id: UserId,
    pub problem_id: i64,
    pub contest_id: i64,
    pub program: String,
    pub language_key: String,
    pub soft_limits: (u64, u64),
    pub op: JobOperation,
}

impl RunManager {
    pub async fn new(
        profile: &Profile,
        config: RunConfig,
        leaderboard_manager: LeaderboardManagerHandle,
        pool: DbPool,
        shutdown: CancellationToken,
    ) -> Result<Self> {
        let (tx, rx) = tokio::sync::broadcast::channel(10);

        let run_data = config
            .languages
            .iter()
            .map(|(k, l)| {
                let mut l = l.clone();
                if let Some(compiled_cmd) = l.runner.compile_cmd.as_mut() {
                    compiled_cmd.setup()?;
                }
                l.runner.run_cmd.setup()?;
                Ok::<(String, LanguageRunnerInfo), anyhow::Error>((k.clone(), l.runner))
            })
            .collect::<Result<_, _>>()
            .context("Failed to initialize language runner data")?;

        let mut isolation_config = config.isolation.clone();
        isolation_config.setup(profile.as_str() == "debug").await?;

        Ok(Self {
            config,
            isolation_config,
            language_runner_info: run_data,
            id_counter: 1,
            leaderboard_handle: leaderboard_manager,
            jobs: HashMap::with_capacity(10),
            db_pool: pool,
            job_started_channel: (tx, rx),
            problem_updated_channels: HashMap::with_capacity(5),
            shutdown,
        })
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

    pub async fn subscribe_shutdown(&self, user_id: &UserId) -> CancellationToken {
        if let Some(handle) = self.jobs.get(user_id) {
            let handle = handle.lock().await;
            if let Some((_, _, shutdown)) = handle.as_ref() {
                shutdown.clone()
            } else {
                self.shutdown.clone()
            }
        } else {
            self.shutdown.clone()
        }
    }

    async fn start_job(&mut self, request: JobRequest) -> Result<(), String> {
        if request.program.len() > self.config.max_program_length {
            return Err(format!(
                "Program too long, max length is {} bytes",
                self.config.max_program_length
            ));
        }

        let user_id = request.user_id;
        let problem_id = request.problem_id;
        let contest_id = request.contest_id;
        let pizzaz = self.config.pizzaz;
        let program = request.program.clone();

        let shutdown = CancellationToken::new();
        let (state_tx, state_rx) = tokio::sync::watch::channel(JobState::new_for_op(&request.op));

        let shutdown_handle = shutdown.clone();

        let handle = Arc::new(Mutex::new(Some((
            problem_id,
            state_rx.clone(),
            shutdown_handle,
        ))));

        self.jobs.insert(user_id, handle.clone());

        let pool = self.db_pool.clone();

        let leaderboard_handle = self.leaderboard_handle.clone();

        let shutdown_job = shutdown.clone();

        let isolation = self.isolation_config.clone();

        tokio::spawn(async move {
            let (state, ran_at) =
                run_job(&request, state_tx, shutdown_job, &isolation, pizzaz).await;

            if !matches!(state, JobState::Judging { .. }) {
                handle.lock().await.take();
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
            handle.lock().await.take();
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
            if let Some((_, _, shutdown)) = handle.as_ref() {
                shutdown.cancel();
            }
        }
    }

    pub async fn shutdown(&mut self) {
        for (_, handle) in self.jobs.drain() {
            let handle = handle.lock().await;
            if let Some((_, _, shutdown)) = handle.as_ref() {
                shutdown.cancel();
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

    fn create_job_request(&mut self, req: ManagerJobRequest) -> Result<JobRequest, String> {
        let language_info = self
            .language_runner_info
            .get(&req.language_key)
            .ok_or_else(|| format!("Language {} not found", req.language_key))?
            .clone();

        let id = self.id_counter;
        self.id_counter += 1;

        Ok(JobRequest {
            id,
            user_id: req.user_id,
            problem_id: req.problem_id,
            contest_id: req.contest_id,
            program: req.program,
            language_key: req.language_key,
            language: language_info,
            soft_limits: req.soft_limits,
            op: req.op,
        })
    }

    pub async fn request_job(&mut self, request: ManagerJobRequest) -> Result<(), String> {
        if let Some(handle) = self.jobs.get(&request.user_id) {
            let handle = handle.lock().await;
            if handle.is_some() {
                Err("User already has a job running".to_string())
            } else {
                drop(handle);
                let req = self.create_job_request(request)?;
                self.start_job(req).await
            }
        } else {
            let req = self.create_job_request(request)?;
            self.start_job(req).await
        }
    }
}
