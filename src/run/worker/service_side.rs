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
    process::{Child, ChildStdin, ChildStdout, Command},
    select,
};

use crate::{
    error::prelude::*,
    problems::TestCase,
    run::{
        config::{CommandInfo, LanguageRunnerInfo},
        job::JobRequest,
        manager::ShutdownReceiver,
    },
};

use super::{
    isolation::{
        self,
        id_map::{map_uid_gid, MapInfo},
        IsolationConfig,
    },
    CaseError, CaseResult, CmdResult, DiagnosticInfo, InitialWorkerInfo, ServiceMessage,
    WorkerMessage,
};

pub struct Worker {
    tmp_dir: PathBuf,
    child: Child,
    sub_child_pid: Option<Pid>,
    shutdown_rx: ShutdownReceiver,
    compile_cmd: Option<CommandInfo>,
    run_cmd: CommandInfo,
    stdin: ChildStdin,
    env: HashMap<String, String>,
    stdout: BufReader<ChildStdout>,
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

    async fn make_temp(parent: Option<&Path>, prefix: &str) -> Result<PathBuf> {
        let now_nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("Couldn't get time since epoch")?
            .as_nanos();
        let default_temp = std::env::temp_dir();
        let temp_dir = parent.unwrap_or(&default_temp);
        let name = format!("{prefix}_{}", now_nanos);
        let temp_path = temp_dir.join(name);
        tokio::fs::create_dir_all(&temp_path)
            .await
            .context("Couldn't create temp directory")?;
        Ok(temp_path)
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

        let map_info = isolation::id_map::get_uid_gid_maps(&iso)
            .await
            .context("Couldn't allocate UID and GID maps")?;

        let name = format!("wcpc_worker_{}", id);
        let tmp_parent = iso.workers_parent.as_deref();
        let tmp_dir = Self::make_temp(tmp_parent, &name)
            .await
            .context("Couldn't create temp directory")?;

        let mut child = Self::create_command(&tmp_dir)?
            .spawn()
            .context("Couldn't spawn worker")?;

        let stdin = child.stdin.take().context("Couldn't take child stdin")?;
        let stdout = child.stdout.take().context("Couldn't take child stdout")?;
        let stdout_reader = BufReader::new(stdout);

        let mut worker = Self {
            tmp_dir,
            compile_cmd: run.compile_cmd.clone(),
            run_cmd: run.run_cmd.clone(),
            env,
            shutdown_rx,
            child,
            sub_child_pid: None,
            stdin,
            stdout: stdout_reader,
        };

        worker.init(req, diag, iso, run, map_info).await?;

        Ok(worker)
    }

    async fn init(
        &mut self,
        req: &JobRequest,
        diag: DiagnosticInfo,
        iso: IsolationConfig,
        run: LanguageRunnerInfo,
        map_info: MapInfo,
    ) -> Result {
        let msg = ServiceMessage::InitialInfo(InitialWorkerInfo {
            diagnostic_info: diag,
            isolation_config: iso,
            program: req.program.to_string(),
            file_name: run.file_name,
        });

        self.send_message(msg).await?;

        let msg = self.wait_for_new_message().await?;

        if let WorkerMessage::RequestUidGidMap(pid) = msg {
            self.sub_child_pid = Some(Pid::from_raw(pid));

            let res = map_uid_gid(pid, map_info).await;

            self.send_message(ServiceMessage::UidGidMapResult(res.is_ok()))
                .await?;
            res.context("Couldn't map UID and GID")?;

            let msg = self.wait_for_new_message().await?;

            if let WorkerMessage::Ready = msg {
                Ok(())
            } else {
                bail!("Unexpected worker response: {:?}", msg);
            }
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
        self.exec_cmd(self.run_cmd.clone(), stdin.map(|s| s.to_string()))
            .await
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
        if self.child.id().is_some() {
            self.send_message(ServiceMessage::Stop).await?;
            self.wait_child().await
        } else {
            Ok(())
        }
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
                self.kill_child().await?;
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
            self.wait_child().await?;
            bail!("Worker internal error: {}", why);
        } else {
            Ok(msg)
        }
    }

    async fn wait_child(&mut self) -> Result {
        // TODO: timeout
        self.child
            .wait()
            .await
            .context("Couldn't wait for worker process")?;
        Ok(())
    }

    fn kill_sub_child(&mut self) -> Result {
        if let Some(pid) = self.sub_child_pid {
            let res = nix::sys::signal::kill(pid, signal::Signal::SIGKILL);
            match res {
                Ok(_) | Err(Errno::ESRCH) => {}
                Err(e) => bail!("Couldn't kill sub-child process: {:?}", e),
            }
        }
        Ok(())
    }

    async fn kill_child(&mut self) -> Result {
        self.kill_sub_child()?;
        if self.child.id().is_some() {
            self.child
                .kill()
                .await
                .context("Couldn't kill worker process")?;
        }
        self.wait_child().await?;
        Ok(())
    }

    fn kill_child_sync(&mut self) -> Result {
        self.kill_sub_child()?;
        if let Some(pid) = self.child.id() {
            let pid = Pid::from_raw(pid as i32);
            nix::sys::signal::kill(pid, signal::Signal::SIGKILL)
                .context("Couldn't kill worker process")?;
            nix::sys::wait::waitpid(pid, None).context("Couldn't wait for worker process")?;
        }
        Ok(())
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
        if self.child.id().is_some() {
            warn!("Worker dropped without being finished, attempting kill...");
        }
        if let Err(why) = self.kill_child_sync() {
            error!("Couldn't kill worker: {:?}", why);
        }
    }
}
