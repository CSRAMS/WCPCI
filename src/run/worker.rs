use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use anyhow::bail;
use chrono::NaiveDateTime;
use log::warn;
use nix::{sys::signal, unistd::Pid};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt},
    select,
};

use crate::error::prelude::*;

use super::{
    job::{Job, JobOperation, JobRequest},
    manager::ShutdownReceiver,
    JobState, JobStateSender,
};

pub struct Worker {
    tmp_path: PathBuf,
    request: JobRequest,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum WorkerMessage {
    StateChange(JobState),
    ChildPid(i32),
    Finished(JobState, NaiveDateTime),
    Failed(String),
}

impl Worker {
    pub async fn new(id: u64, request: JobRequest) -> Result<Self> {
        let tmp_path = super::make_temp(&format!("wcpc_worker_{}", id))
            .await
            .context("Couldn't create temp directory")?;
        Ok(Worker { tmp_path, request })
    }

    pub async fn spawn(
        self,
        state_tx: JobStateSender,
        op: &JobOperation,
        shutdown_rx: ShutdownReceiver,
    ) -> (JobState, NaiveDateTime) {
        let res = self._spawn(&state_tx, shutdown_rx).await;
        tokio::fs::remove_dir_all(&self.tmp_path)
            .await
            .unwrap_or_else(|why| {
                warn!("Couldn't remove temp directory: {:?}", why);
            });
        match res {
            Ok(res) => res,
            Err(why) => {
                let mut state = JobState::new_for_op(op);
                error!("Worker failed to start: {why:?}");
                state.force_fail("JudgeError: Worker failed to start");
                state_tx.send(state.clone()).ok();
                (state, chrono::Utc::now().naive_utc())
            }
        }
    }

    async fn _spawn(
        &self,
        state_tx: &JobStateSender,
        mut shutdown_rx: ShutdownReceiver,
    ) -> Result<(JobState, NaiveDateTime)> {
        let self_path = std::env::current_exe().context("Couldn't get current executable path")?;

        let mut child = tokio::process::Command::new(self_path)
            .arg("--worker")
            .env_clear()
            .envs(self.request.language.environment.iter())
            .env("PATH", &self.request.language.path_var) // TODO(Ellis): look at later
            .current_dir(&self.tmp_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .kill_on_drop(true)
            .spawn()
            .context("Couldn't spawn worker process")?;

        let mut stdin = child.stdin.take().context("Couldn't get child stdin")?;

        let req =
            serde_json::to_string(&self.request).context("Couldn't serialize job request")? + "\n";

        stdin
            .write_all(req.as_bytes())
            .await
            .context("Couldn't write job request to child")?;

        drop(stdin);

        let stdout = child.stdout.take().context("Couldn't get child stdout")?;

        let mut stdout_reader = tokio::io::BufReader::new(stdout);

        let mut buf = String::new();

        let mut child_pid: Option<i32> = None;

        let mut last_state = JobState::new_for_op(&self.request.op);

        loop {
            select! {

                biased;

                _ = stdout_reader.read_line(&mut buf) => {
                    let msg = serde_json::from_str::<WorkerMessage>(&buf);
                    match msg {
                        Ok(WorkerMessage::StateChange(state)) => {
                            last_state = state.clone();
                            let res = state_tx.send(state).context("Couldn't send job state");
                            if let Err(why) = res {
                                warn!("Couldn't send job state: {}", why);
                            }
                        },
                        Ok(WorkerMessage::ChildPid(pid)) => {
                            child_pid = Some(pid);
                        },
                        Ok(WorkerMessage::Finished(state, dt)) => {
                            return Ok((state, dt));
                        },
                        Ok(WorkerMessage::Failed(why)) => {
                            last_state.force_fail("JudgeError: Worker process failed");
                            state_tx.send(last_state.clone()).context("Couldn't send job state")?;
                            error!("Worker process failed: {}", why);
                            child.wait().await.context("Couldn't wait for worker process")?;
                            return Ok((last_state, chrono::Utc::now().naive_utc()));
                        },
                        Err(why) => {
                            warn!("Couldn't deserialize worker message:\n{}\n\nMessage: \"{}\"", why, &buf);
                        }
                    }
                    buf.clear();
                },
                _ = child.wait(), if child_pid.is_none() => {
                    last_state.force_fail("Worker process exited unexpectedly");
                    state_tx.send(last_state.clone()).context("Couldn't send job state")?;
                    stdout_reader.read_to_string(&mut buf).await.context("[E] Couldn't read child stdout")?;
                    bail!("Worker process exited unexpectedly:\n\n{buf}");
                },
                _ = shutdown_rx.changed() => {
                    last_state.force_fail("Run Cancelled");
                    state_tx.send(last_state.clone()).context("Couldn't send job state")?;
                    child.kill().await.context("Couldn't kill worker process")?;
                    if let Some(child_pid) = child_pid {
                        let pid = Pid::from_raw(child_pid);
                        nix::sys::signal::kill(pid, signal::SIGKILL)
                            .context("Couldn't kill worker process")?;
                    }
                    return Ok((last_state, chrono::Utc::now().naive_utc()));
                }
            }
        }
    }

    pub fn run_from_child(dir: &Path) -> Result {
        let stdin = std::io::stdin();
        let mut buffer = String::new();
        let mut stdin_reader = BufReader::new(stdin);

        stdin_reader
            .read_line(&mut buffer)
            .context("Couldn't read job request")?;

        let request = serde_json::from_str::<JobRequest>(&buffer)
            .context("Couldn't deserialize job request")?;

        let mounts = super::lockdown::lockdown_process(&request, dir)
            .context("Couldn't lockdown worker process")?;

        drop(stdin_reader);

        info!("Worker process started and locked");

        let job = Job::new(request).context("Couldn't create job")?;

        let (state, completed_at) = job.run();

        let res = WorkerMessage::Finished(state, completed_at);
        let res = serde_json::to_string(&res).context("Couldn't serialize job result")?;

        for mount in mounts {
            if let Err(why) = mount.unmount().context("Couldn't unmount bind mount") {
                error!("Couldn't unmount bind mount: {:?}", why);
            }
        }

        println!("{}", res);

        Ok(())
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if self.tmp_path.exists() {
            let dir = self
                .tmp_path
                .read_dir()
                .map(|d| d.collect::<Vec<_>>())
                .unwrap_or_default();
            if !dir.is_empty() {
                warn!("Temp directory {:?} not empty, contents:", dir);
                for entry in dir {
                    warn!("- {:?}", entry);
                }
            }
            std::fs::remove_dir_all(&self.tmp_path).unwrap_or_else(|why| {
                warn!("Couldn't remove temp directory: {:?}", why);
            });
        }
    }
}

use log::{Metadata, Record};

pub struct WorkerLogger(String);

impl WorkerLogger {
    fn new() -> Self {
        let cwd = std::env::current_dir().unwrap();
        let name = cwd.file_name().unwrap().to_string_lossy().to_string();
        let number = name.split('_').nth(2).unwrap_or("?");
        Self(format!("Worker Run #{number}"))
    }

    pub fn setup() {
        let logger = WorkerLogger::new();
        let level = if cfg!(debug_assertions) {
            log::LevelFilter::Debug
        } else {
            log::LevelFilter::Info
        };
        log::set_max_level(level);
        log::set_boxed_logger(Box::new(logger)).unwrap();
    }
}

impl log::Log for WorkerLogger {
    fn enabled(&self, _metadata: &Metadata) -> bool {
        true
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[{}][{}]: {}", &self.0, record.level(), record.args());
        }
    }

    fn flush(&self) {}
}
