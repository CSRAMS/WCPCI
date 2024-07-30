use std::{
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use chrono::NaiveDateTime;
use log::warn;
use nix::{errno::Errno, sys::signal, unistd::Pid};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt},
    select,
};

use crate::{error::prelude::*, problems::TestCase, run::lockdown};

use super::{
    job::{Job, JobRequest},
    manager::ShutdownReceiver,
    JobState, JobStateSender,
};

pub struct Worker {
    id: u64,
    tmp_path: PathBuf,
    request: JobRequest,
    state: JobState,
    child_pid: Option<i32>,
    state_tx: JobStateSender,
    test_cases: Vec<TestCase>,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum WorkerMessage {
    StateChange(JobState),
    RequestCheck(String),
    ChildPid(i32),
    Finished(JobState, NaiveDateTime),
    Failed(String),
}

impl Worker {
    pub async fn new(
        id: u64,
        request: JobRequest,
        test_cases: Vec<TestCase>,
        state_tx: JobStateSender,
    ) -> Result<Self> {
        let tmp_path = super::make_temp(&format!("wcpc_worker_{}", id))
            .await
            .context("Couldn't create temp directory")?;
        let state = JobState::new_for_op(&request.op);
        Ok(Worker {
            id,
            tmp_path,
            request,
            test_cases,
            state,
            state_tx,
            child_pid: None,
        })
    }

    pub async fn spawn(mut self, shutdown_rx: ShutdownReceiver) -> (JobState, NaiveDateTime) {
        let res = self._spawn(shutdown_rx).await;
        tokio::fs::remove_dir_all(&self.tmp_path)
            .await
            .unwrap_or_else(|why| {
                warn!("Couldn't remove temp directory: {:?}", why);
            });
        match res {
            Ok(res) => res,
            Err(why) => {
                self.state.force_fail(&format!("Worker error: {:?}", why));
                self.publish_state().await;
                self.get_return().unwrap()
            }
        }
    }

    async fn _spawn(
        &mut self,
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

        let stdout = child.stdout.take().context("Couldn't get child stdout")?;

        let mut stdout_reader = tokio::io::BufReader::new(stdout);

        let mut buf = String::new();

        let mut current_idx = 0;

        loop {
            select! {

                biased;

                _ = stdout_reader.read_line(&mut buf) => {
                    let msg = serde_json::from_str::<WorkerMessage>(&buf);
                    match msg {
                        Ok(WorkerMessage::StateChange(state)) => {
                            self.receive_worker_state(state).await?;
                        },
                        Ok(WorkerMessage::ChildPid(pid)) => {
                            if self.child_pid.is_some() {
                                warn!("Worker {}: Received multiple child PIDs, ignoring", self.id);
                                continue;
                            }
                            self.child_pid = Some(pid);
                            match lockdown::map_ids(pid) {
                                Ok(_) => {
                                    stdin.write_all(b"y\n").await.context("Couldn't write newline to child")?;
                                },
                                Err(why) => {
                                    stdin.write_all(b"n\n").await.context("Couldn't write newline to child")?;
                                    self.judge_error(&format!("Couldn't map IDs: {:?}", &why), &mut child).await?;
                                    return self.get_return();
                                }
                            }
                        },
                        Ok(WorkerMessage::RequestCheck(output)) => {
                            if self.state.is_testing() {
                                stdin.write_all(b"y\n").await.context("Couldn't write newline to child")?;
                            }

                            if current_idx >= self.test_cases.len() {
                                warn!("Worker {}: Received more checks than expected", self.id);
                                warn!("Possible tampering? Ignoring...");
                                stdin.write_all(b"y\n").await.context("Couldn't write newline to child")?;
                                continue;
                            }
                            let case = &self.test_cases.get(current_idx).context("Couldn't get test case")?;
                            match case.check_output(&output) {
                                Ok(correct) => {
                                    let yn = if correct { "y\n" } else { "n\n" };
                                    stdin.write_all(yn.as_bytes()).await.context("Couldn't write newline to child")?;
                                    current_idx += 1;
                                },
                                Err(why) => {
                                    stdin.write_all(b"y\n").await.context("Couldn't write newline to child")?;
                                    self.judge_error(&format!("Couldn't check output: {:?}", &why), &mut child).await?;
                                    return self.get_return();
                                }
                            }
                        },
                        // TODO: Is it okay to ignore dt here?
                        // This means that times will be slightly influenced by
                        // how fast the worker can give back the message,
                        // potentially we can measure the difference between the two
                        // and have a threshold for how off it can be
                        Ok(WorkerMessage::Finished(state, _dt)) => {
                            let dt = chrono::Utc::now().naive_utc();
                            self.receive_worker_state(state).await?;
                            self.state.force_complete();
                            self.publish_state().await;
                            self.wait_for_child(&mut child).await?;
                            return Ok((self.state.clone(), dt));
                        },
                        Ok(WorkerMessage::Failed(why)) => {
                            self.judge_error(&why, &mut child).await?;
                            return self.get_return();
                        },
                        Err(why) => {
                            warn!("Worker {}: Couldn't deserialize worker message:\n{}\n\nMessage: \"{}\"", self.id, why, &buf);
                        }
                    }
                    buf.clear();
                },
                _ = child.wait(), if self.child_pid.is_none() => {
                    stdout_reader.read_to_string(&mut buf).await.ok();
                    let output = format!("Worker process exited without Finished message, stdout:\n{}", &buf);
                    self.judge_error(&output, &mut child).await?;
                    return self.get_return();
                },
                _ = shutdown_rx.changed() => {
                    self.state.force_fail("Run Cancelled");
                    self.publish_state().await;
                    self.kill_child(&mut child).await?;
                    return self.get_return();
                }
            }
        }
    }

    async fn publish_state(&self) {
        let res = self.state_tx.send(self.state.clone());
        if let Err(why) = res {
            warn!("Couldn't send job state: {:?}", why);
        }
    }

    async fn judge_error(&mut self, msg: &str, child: &mut tokio::process::Child) -> Result {
        error!("Worker {} process has judge error: {}", self.id, msg);
        self.state.force_fail("Judge Error");
        self.publish_state().await;
        self.wait_for_child(child).await?;
        Ok(())
    }

    fn get_return(&self) -> Result<(JobState, chrono::NaiveDateTime)> {
        let mut state = self.state.clone();
        state.force_complete();
        Ok((state, chrono::Utc::now().naive_utc()))
    }

    async fn receive_worker_state(&mut self, mut state: JobState) -> Result {
        if state.is_judging() {
            if state.len() != self.test_cases.len() {
                warn!(
                    "Worker {} state has different number of cases than expected: {} vs {}",
                    self.id,
                    state.len(),
                    self.test_cases.len()
                );
                warn!("Possible tampering? Ignoring...");
                return Ok(());
            }
            // Ensure a potentially compromised stdout is still checked by us
            // Checks if the output is correct a second time to ensure the worker
            // didn't tamper with the output
            state.check_against_cases(&self.test_cases)?;
        }

        // If we're judging we don't want error messages with details to get back to
        // the user, as they have the potential to expose test cases amongst others.
        // So we "limit" failures, meaning it will only show a simple message
        // instead, in addition if a failure is already limited we mask the error and
        // simply log it, as it could be someone attempting to exfiltrate data by
        // setting the error message to something they want to see like input.
        // This also prevents judge errors from being shown to the user when
        // testing.
        state.limit_all_failures();

        self.state = state;
        self.publish_state().await;
        Ok(())
    }

    async fn wait_for_child(&mut self, child: &mut tokio::process::Child) -> Result {
        child
            .wait()
            .await
            .context("Couldn't wait for worker process")?;
        if let Some(child_pid) = self.child_pid {
            let pid = Pid::from_raw(child_pid);
            if let Err(why) = nix::sys::wait::waitpid(pid, None) {
                if !matches!(why, Errno::ECHILD) {
                    warn!("Couldn't wait for child process: {}. Force killing...", why);
                    nix::sys::signal::kill(pid, signal::SIGKILL)
                        .context("Couldn't kill worker process")?;
                }
            }
        }
        Ok(())
    }

    async fn kill_child(&mut self, child: &mut tokio::process::Child) -> Result {
        child.kill().await.context("Couldn't kill worker process")?;
        if let Some(child_pid) = self.child_pid {
            let pid = Pid::from_raw(child_pid);
            nix::sys::signal::kill(pid, signal::SIGKILL).context("Couldn't kill worker process")?;
        }
        Ok(())
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

        let _handle = super::lockdown::lockdown_process(&request, dir)
            .context("Couldn't lockdown worker process")?;

        drop(stdin_reader);

        info!("Worker process started and locked");

        let job = Job::new(request).context("Couldn't create job")?;

        let (state, completed_at) = job.run();

        let res = WorkerMessage::Finished(state, completed_at);
        let res = serde_json::to_string(&res).context("Couldn't serialize job result")?;

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
        let logger = Self::new();
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
