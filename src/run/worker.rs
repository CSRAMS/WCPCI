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
    manager::ShutdownReceiver,
    JobState, JobStateSender,
};

pub struct Worker {
    request: JobRequest,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum WorkerMessage {
    StateChange(JobState),
    Finished(JobState, NaiveDateTime),
    Failed(String),
}

impl Worker {
    pub fn new(req: JobRequest) -> Self {
        Worker { request: req }
    }

    pub async fn spawn(
        &self,
        state_tx: JobStateSender,
        op: &JobOperation,
        shutdown_rx: ShutdownReceiver,
    ) -> (JobState, NaiveDateTime) {
        let res = self._spawn(&state_tx, shutdown_rx).await;
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
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
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

    pub async fn run_from_child() -> Result {
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

        Ok(())
    }
}
