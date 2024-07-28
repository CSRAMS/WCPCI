use std::path::{Path, PathBuf};

use anyhow::bail;
use chrono::NaiveDateTime;
use log::warn;
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    select,
};

use crate::error::prelude::*;

use super::{
    job::{Job, JobOperation, JobRequest},
    lockdown::BindMount,
    manager::ShutdownReceiver,
    JobState, JobStateSender,
};

pub struct Worker {
    tmp_path: PathBuf,
    request: JobRequest,
    mounts: Vec<BindMount>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum WorkerMessage {
    StateChange(JobState),
    Finished(JobState, NaiveDateTime),
    Failed(String),
}

impl Worker {
    pub async fn new(id: u64, request: JobRequest) -> Result<Self> {
        let tmp_path = super::make_temp(&format!("wcpc_worker_{}", id))
            .await
            .context("Couldn't create temp directory")?;
        let mut mounts = Vec::with_capacity(request.language.expose_paths.len());
        for path in request.language.expose_paths.iter() {
            let mount = BindMount::new(&tmp_path, path)
                .await
                .context("Couldn't create bind mount")?;
            mounts.push(mount);
        }
        Ok(Worker {
            tmp_path,
            request,
            mounts,
        })
    }

    fn unmount_all(&self) {
        for mount in &self.mounts {
            mount.unmount().unwrap_or_else(|why| {
                warn!("Couldn't unmount bind mount: {:?}", why);
            });
        }
    }

    pub async fn spawn(
        self,
        state_tx: JobStateSender,
        op: &JobOperation,
        shutdown_rx: ShutdownReceiver,
    ) -> (JobState, NaiveDateTime) {
        let res = self._spawn(&state_tx, shutdown_rx).await;
        self.unmount_all();
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
            .env("PATH", &self.request.language.path_var)
            .current_dir(&self.tmp_path)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::inherit())
            .spawn()
            .context("Couldn't spawn worker process")?;

        let mut stdin = child.stdin.take().context("Couldn't get child stdin")?;

        let req =
            serde_json::to_string(&self.request).context("Couldn't serialize job request")? + "\n";

        stdin
            .write_all(req.as_bytes())
            .await
            .context("Couldn't write job request to child")?;

        let stdout = child.stdout.take().context("Couldn't get child stdout")?;

        let mut stdout_reader = tokio::io::BufReader::new(stdout);

        let mut buf = String::new();

        loop {
            select! {

                biased;

                _ = stdout_reader.read_line(&mut buf) => {
                    let msg = serde_json::from_str::<WorkerMessage>(&buf);
                    match msg {
                        Ok(WorkerMessage::StateChange(state)) => {
                            let res = state_tx.send(state).context("Couldn't send job state");
                            if let Err(why) = res {
                                warn!("Couldn't send job state: {}", why);
                            }
                        },
                        Ok(WorkerMessage::Finished(state, dt)) => {
                            return Ok((state, dt));
                        },
                        Ok(WorkerMessage::Failed(why)) => {
                            bail!("Worker process failed: {}", why);
                        },
                        Err(why) => {
                            warn!("Couldn't deserialize worker message: {}", why);
                        }
                    }
                    buf.clear();
                },
                _ = child.wait() => {
                    stdout_reader.read_to_string(&mut buf).await.context("[E] Couldn't read child stdout")?;
                    bail!("Worker process exited unexpectedly:\n\n{buf}");
                },
                _ = shutdown_rx.changed() => {
                    stdin.write_all(b"shutdown\n").await.context("Couldn't send shutdown to worker")?;
                }
            }
        }
    }

    pub async fn run_from_child(dir: &Path) -> Result {
        let _handle = super::lockdown::lockdown_process(dir)
            .await
            .context("Couldn't lockdown worker process")?;

        let stdin = tokio::io::stdin();
        let mut buffer = String::new();
        let mut stdin_reader = BufReader::new(stdin);

        stdin_reader
            .read_line(&mut buffer)
            .await
            .context("Couldn't read job request")?;

        let request = serde_json::from_str::<JobRequest>(&buffer)
            .context("Couldn't deserialize job request")?;

        let (tx, rx) = tokio::sync::watch::channel(false);

        let job = Job::new(request, rx).await.context("Couldn't create job")?;

        let handle = tokio::spawn(async move { job.run().await });

        let mut shutdown_triggered = false;
        const SHUTDOWN_TIMEOUT: tokio::time::Duration = tokio::time::Duration::from_millis(3000);

        tokio::pin!(handle);

        loop {
            select! {
                biased; // Prefer the first branch if the job completed

                res = handle.as_mut() => {
                    let (state, completed_at) = res.context("Couldn't get job result")?;
                    let msg = WorkerMessage::Finished(state, completed_at);
                    let res = serde_json::to_string(&msg).context("Couldn't serialize job state")?;
                    println!("{}", res);
                    break;
                },
                _ = stdin_reader.read_line(&mut buffer) => {
                    shutdown_triggered = true;
                    tx.send(true).ok();
                },
                _ = tokio::time::sleep(SHUTDOWN_TIMEOUT), if shutdown_triggered => {
                    "Shutdown timeout".to_string();
                }
            }
        }

        drop(_handle);

        Ok(())
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if self.tmp_path.exists() {
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
        Self(cwd.to_string_lossy().to_string())
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
