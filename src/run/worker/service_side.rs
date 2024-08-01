use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    process::Stdio,
    time::Duration,
};

use anyhow::bail;
use nix::{errno::Errno, sys::signal, unistd::Pid};
use tokio::{
    io::{AsyncBufReadExt, AsyncWriteExt, BufReader},
    process::{ChildStdin, ChildStdout, Command},
    select,
};

use crate::{
    error::prelude::*,
    problems::TestCase,
    run::{
        config::{CommandInfo, LanguageRunnerInfo},
        isolation::{self, IsolationConfig},
        job::JobRequest,
        manager::ShutdownReceiver,
    },
};

use super::{
    CaseError, CaseResult, CmdResult, DiagnosticInfo, InitialWorkerInfo, ServiceMessage,
    WorkerMessage,
};

pub struct Worker {
    tmp_dir: PathBuf,
    child_pid: Pid,
    shutdown_rx: ShutdownReceiver,
    compile_cmd: Option<CommandInfo>,
    run_cmd: CommandInfo,
    stdin: ChildStdin,
    env: HashMap<String, String>,
    stdout: BufReader<ChildStdout>,
    child_done: bool,
}

impl Worker {
    fn create_command(tmp_dir: &Path) -> Result<Command> {
        let self_exe = std::env::current_exe().context("Couldn't get current executable path")?;
        let mut cmd = Command::new(self_exe);
        cmd.arg("--worker")
            .env_clear()
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .current_dir(tmp_dir);
        Ok(cmd)
    }

    async fn make_temp(prefix: &str) -> Result<PathBuf> {
        let now_nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("Couldn't get time since epoch")?
            .as_nanos();
        let temp_dir = std::env::temp_dir();
        let name = format!("{prefix}_{}", now_nanos);
        let temp_path = temp_dir.join(name);
        tokio::fs::create_dir_all(&temp_path)
            .await
            .context("Couldn't create temp directory")?;
        Ok(temp_path)
    }

    async fn setup_uid_gid(&mut self) -> Result {
        let res = isolation::id_map::map_uid_gid(self.child_pid.as_raw())
            .await
            .context("Couldn't map UID and GID");
        self.send_message(ServiceMessage::UidGidMapResult(res.is_ok()))
            .await?;
        res
    }

    pub async fn new(
        id: u64,
        req: &JobRequest,
        shutdown_rx: ShutdownReceiver,
        run: LanguageRunnerInfo,
        iso: IsolationConfig,
        diag: DiagnosticInfo,
    ) -> Result<Self> {
        let mut env = iso.env.clone();
        env.extend(run.env.clone());

        let name = format!("wcpc_worker_{}", id);
        let tmp_dir = Self::make_temp(&name)
            .await
            .context("Couldn't create temp directory")?;

        let msg = ServiceMessage::InitialInfo(InitialWorkerInfo {
            diagnostic_info: diag,
            isolation_config: iso,
            program: req.program.to_string(),
            file_name: run.file_name,
        })
        .serialize()?;
        let msg = format!("{}\n", msg);

        let mut child = Self::create_command(&tmp_dir)?
            .spawn()
            .context("Couldn't spawn worker")?;

        let stdout = child.stdout.take().context("Couldn't get worker stdout")?;
        let mut stdout_reader = tokio::io::BufReader::new(stdout);

        let mut stdin = child.stdin.take().context("Couldn't get worker stdin")?;
        stdin
            .write_all(msg.as_bytes())
            .await
            .context("Couldn't write initial message to worker")?;

        let mut buf = String::new();
        stdout_reader
            .read_line(&mut buf)
            .await
            .context("Couldn't read worker response")?;

        let msg: WorkerMessage =
            serde_json::from_str(&buf).context("Couldn't deserialize worker response")?;

        if let WorkerMessage::RequestUidGidMap(pid) = msg {
            child.wait().await.context("Couldn't wait for worker")?;

            let mut worker = Self {
                tmp_dir,
                compile_cmd: run.compile_cmd.clone(),
                run_cmd: run.run_cmd.clone(),
                env,
                shutdown_rx,
                child_pid: Pid::from_raw(pid),
                stdin,
                stdout: stdout_reader,
                child_done: false,
            };

            worker.setup_uid_gid().await?;
            let msg = worker.wait_for_new_message().await?;

            if let WorkerMessage::Ready = msg {
                Ok(worker)
            } else {
                bail!("Unexpected worker response: {:?}", msg);
            }
        } else if let WorkerMessage::InternalError(why) = msg {
            bail!("Worker internal error: {:?}", why);
        } else {
            bail!("Unexpected worker response: {:?}", msg);
        }
    }

    pub async fn compile(&mut self) -> CaseResult {
        if let Some(cmd) = self.compile_cmd.clone() {
            self.exec_cmd(cmd, None)
                .await
                .map_err(|e| match e {
                    CaseError::Runtime(failure) => CaseError::Compilation(failure),
                    e => e,
                })
                .map(|_| ())
        } else {
            Ok(())
        }
    }

    pub async fn run_cmd(&mut self, stdin: Option<&str>) -> CaseResult<String> {
        // Sleep for a bit of pizzaz
        tokio::time::sleep(Duration::from_millis(250)).await;

        let mut shutdown_rx = self.shutdown_rx.clone();

        select! {
            res = self.exec_cmd(self.run_cmd.clone(), stdin.map(|s| s.to_string())) => {
                res
            }
            _ = shutdown_rx.changed() => {
                self.kill_child()?;
                Err(CaseError::Cancelled)
            }
        }
    }

    pub async fn run_case(&mut self, case: &TestCase) -> CaseResult<String> {
        self.run_cmd(Some(&case.stdin)).await.and_then(|output| {
            let correct = case.check_output(&output).map_err(CaseError::Judge)?;
            if correct {
                Ok(output)
            } else {
                Err(CaseError::Logic)
            }
        })
    }

    pub async fn finish(mut self) -> Result {
        self.send_message(ServiceMessage::Stop).await?;
        self.wait_child()
    }

    async fn exec_cmd(&mut self, cmd: CommandInfo, stdin: Option<String>) -> CaseResult<String> {
        let msg = ServiceMessage::RunCmd(cmd.clone(), stdin, self.env.clone());
        self.send_message(msg).await?;
        let mut shutdown_rx = self.shutdown_rx.clone();

        select! {
            msg = self.wait_for_new_message() => {
                if let WorkerMessage::CmdComplete(res) = msg? {
                    match res {
                        CmdResult::Success(output) => {
                            let output = output.stdout;
                            Ok(output)
                        }
                        CmdResult::Failure(failure) => Err(CaseError::Runtime(failure.to_string())),
                    }
                } else {
                    Err(CaseError::Judge("Unexpected worker response".to_string()))
                }
            }
            _ = shutdown_rx.changed() => {
                self.kill_child()?;
                Err(CaseError::Cancelled)
            }
        }
    }

    async fn send_message(&mut self, msg: ServiceMessage) -> Result {
        let msg = format!("{}\n", msg.serialize()?);
        self.stdin
            .write_all(msg.as_bytes())
            .await
            .context("Couldn't write message to worker")
    }

    async fn wait_for_new_message(&mut self) -> Result<WorkerMessage> {
        let mut buf = String::new();
        self.stdout
            .read_line(&mut buf)
            .await
            .context("Couldn't read worker message")?;
        let msg = serde_json::from_str(&buf).context("Couldn't deserialize worker message")?;
        if let WorkerMessage::InternalError(why) = msg {
            self.wait_child()?;
            bail!("Worker internal error: {:?}", why);
        } else {
            Ok(msg)
        }
    }

    fn wait_child(&mut self) -> Result {
        if self.child_done {
            return Ok(());
        }

        if let Err(why) = nix::sys::wait::waitpid(self.child_pid, None) {
            if matches!(why, Errno::ECHILD) {
                self.child_done = true;
            } else {
                warn!("Couldn't wait for child process: {}\nForce killing...", why);
                self.kill_child()?;
            }
        } else {
            self.child_done = true;
        }

        Ok(())
    }

    fn kill_child(&mut self) -> Result {
        if self.child_done {
            return Ok(());
        }

        let res = nix::sys::signal::kill(self.child_pid, signal::SIGKILL);
        match res {
            Ok(_) => {
                self.child_done = true;
                Ok(())
            }
            Err(Errno::ESRCH) => Ok(()),
            Err(why) => Err(why).context("Couldn't kill worker process"),
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        if self.tmp_dir.exists() {
            let res = std::fs::remove_dir_all(&self.tmp_dir).context("Couldn't remove temp dir");
            if let Err(e) = res {
                error!("{e:?}");
            }
        }
        if !self.child_done {
            let res = self.kill_child();
            if let Err(e) = res {
                error!("{e:?}");
            }
        }
    }
}
