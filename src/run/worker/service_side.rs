use std::{
    collections::HashMap,
    future::Future,
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
use tokio_util::sync::CancellationToken;

use crate::{
    error::prelude::*,
    problems::TestCase,
    run::config::{CommandInfo, LanguageRunnerInfo},
};

use super::{
    isolation::{
        self,
        id_map::{map_uid_gid, MapInfo},
        CGroup, CGroupStats, IsolationConfig,
    },
    CaseError, CaseResult, CmdResult, InitialWorkerInfo, ServiceMessage, WorkerMessage,
};

pub struct Worker {
    tmp_dir: PathBuf,
    child: Child,
    sub_child_pid: Option<Pid>,
    cgroup: CGroup,
    last_stat: CGroupStats,
    // CPU time, memory usage
    soft_limits: (u64, u64),
    shutdown: CancellationToken,
    compile_cmd: Option<CommandInfo>,
    run_cmd: CommandInfo,
    stdin: ChildStdin,
    env: HashMap<String, String>,
    stdout: BufReader<ChildStdout>,
    timeout_internal: Duration,
    timeout_user: Duration,
}

enum WaitForResult<T> {
    Ok(T),
    Cancelled,
    HardTimeout,
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

    fn make_temp_name(prefix: &str) -> Result<String> {
        let now_nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .context("Couldn't get time since epoch")?
            .as_nanos();
        Ok(format!("{prefix}_{}", now_nanos))
    }

    async fn make_temp(parent: Option<&Path>, name: &str) -> Result<PathBuf> {
        let default_temp = std::env::temp_dir();
        let temp_dir = parent.unwrap_or(&default_temp);
        let temp_path = temp_dir.join(name);
        tokio::fs::create_dir_all(&temp_path)
            .await
            .context("Couldn't create temp directory")?;
        Ok(temp_path)
    }

    pub async fn new(
        id: u64,
        program: &str,
        shutdown: CancellationToken,
        run: LanguageRunnerInfo,
        iso: IsolationConfig,
        diag: &str,
        soft_limits: (u64, u64),
    ) -> Result<Self> {
        let mut env = iso.env.clone();
        env.extend(run.env.clone());

        let map_info = isolation::id_map::get_uid_gid_maps(&iso)
            .await
            .context("Couldn't allocate UID and GID maps")?;

        let name = Self::make_temp_name(format!("wcpc_worker_{}", id).as_str())?;

        let root_cgroup = iso
            .cgroups
            .as_ref()
            .map(|(r, _)| r)
            .context("No cgroup root")?;
        let cgroup = root_cgroup.create_child(&name, true).await?;
        cgroup.apply_hard_limits(&iso.limits).await?;

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

        let (timeout_internal, timeout_user) = (
            Duration::from_secs(iso.limits.hard_timeout_internal_secs),
            Duration::from_secs(iso.limits.hard_timeout_user_secs),
        );

        let mut worker = Self {
            tmp_dir,
            compile_cmd: run.compile_cmd.clone(),
            run_cmd: run.run_cmd.clone(),
            env,
            shutdown,
            child,
            sub_child_pid: None,
            cgroup,
            soft_limits,
            last_stat: CGroupStats::default(),
            stdin,
            stdout: stdout_reader,
            timeout_internal,
            timeout_user,
        };

        let res = worker.init(program, diag, iso, run, map_info).await;

        if let Err(e) = res {
            worker.finish().await?;
            Err(e)
        } else {
            Ok(worker)
        }
    }

    async fn init(
        &mut self,
        program: &str,
        diag: &str,
        iso: IsolationConfig,
        run: LanguageRunnerInfo,
        map_info: MapInfo,
    ) -> Result {
        let pid = self.child.id().context("Worker process has no PID")?;
        self.cgroup
            .move_pid(pid as i32)
            .await
            .context("Couldn't move PID to cgroup")?;

        let msg = ServiceMessage::InitialInfo(InitialWorkerInfo {
            diagnostic_info: diag.to_string(),
            isolation_config: iso,
            program: program.to_string(),
            file_name: run.file_name,
        });

        self.send_message(msg).await?;

        let msg = self.wait_for_new_message(None).await?;

        if let WorkerMessage::RequestUidGidMap(pid) = msg {
            self.sub_child_pid = Some(Pid::from_raw(pid));

            // self.cgroup
            //     .move_pid(pid as i32)
            //     .await
            //     .context("Couldn't move PID to cgroup")?;

            let res = map_uid_gid(pid, map_info).await;

            self.send_message(ServiceMessage::UidGidMapResult(res.is_ok()))
                .await?;
            res.context("Couldn't map UID and GID")?;

            let msg = self.wait_for_new_message(None).await?;

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
            self.exec_cmd(cmd, None, false)
                .await
                .map_err(|e| match e {
                    CaseError::Runtime(failure) => CaseError::Compilation(failure),
                    e => e,
                })
                .map(|_| ())?;
        }
        Ok(())
    }

    pub async fn run_cmd(&mut self, stdin: Option<&str>) -> CaseResult<String> {
        self.cgroup
            .apply_soft_limits(self.soft_limits.0, self.soft_limits.1 * 1024 * 1024)
            .await?;
        // Sleep for a bit of pizzaz
        tokio::time::sleep(Duration::from_millis(250)).await;
        self.exec_cmd(self.run_cmd.clone(), stdin.map(|s| s.to_string()), true)
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
            if self.send_message(ServiceMessage::Stop).await.is_err() {
                self.kill_child().await?;
            } else {
                self.wait_child().await?;
            }
        }
        Ok(())
    }

    async fn exec_cmd(
        &mut self,
        cmd: CommandInfo,
        stdin: Option<String>,
        track_stats: bool,
    ) -> CaseResult<String> {
        let res = self._exec_cmd(cmd, stdin, track_stats).await;
        match res {
            Err(e) if e.should_kill_worker() => {
                info!("Killing worker due to error: {:?}", e);
                self.kill_child().await?;
                Err(e)
            }
            o => o,
        }
    }

    async fn _exec_cmd(
        &mut self,
        cmd: CommandInfo,
        stdin: Option<String>,
        track_stats: bool,
    ) -> CaseResult<String> {
        let msg = ServiceMessage::RunCmd(cmd.clone(), stdin, self.env.clone());

        if track_stats {
            self.update_stats().await?;
        }

        // Clone needed here due to us holding a mutable reference to self while also
        // needing an immutable reference to self.cgroup to check stats during program
        // runtime
        let mut cgroup = self.cgroup.clone();
        cgroup.ephemeral = false; // Don't delete cgroup on drop
        let base_stats = self.last_stat;
        let cpu_limit = self.soft_limits.0;
        let shutdown = self.shutdown.clone();

        self.send_message(msg).await?;

        let future = self.wait_for_new_message(Some(self.timeout_user));

        tokio::pin!(future);

        let res = loop {
            select! {
                biased;
                res = &mut future => {
                    let msg = res?;
                    break match msg {
                        WorkerMessage::CmdComplete(res) => match res {
                            CmdResult::Success(output) => {
                                if track_stats {
                                    let diff = cgroup.get_stats().await? - base_stats;
                                    Self::check_stat_diff(diff, &cgroup, cpu_limit).await
                                } else {
                                    Ok(())
                                }.map(|_| output.stdout)
                            },
                            CmdResult::Failure(failure) => Err(CaseError::Runtime(failure.to_string())),
                        },
                        WorkerMessage::Cancelled => Err(CaseError::Cancelled),
                        WorkerMessage::TimedOut => Err(CaseError::HardTimeLimitExceeded),
                        _ => Err(anyhow!("Unexpected worker response: {:?}", msg).into()),
                    }
                }
                // This branch cannot from the function with an error, as it would
                // result in the worker future never having a shutdown signal sent
                // meaning it could hang indefinitely
                _ = tokio::time::sleep(Duration::from_millis(100)), if track_stats => {
                    let res = cgroup.get_stats().await;
                    match res {
                        Ok(stats) => {
                            let diff = stats - base_stats;
                            if let Err(e) = Self::check_stat_diff(diff, &cgroup, cpu_limit).await {
                                break Err(e);
                            }
                        },
                        Err(e) => {
                            break Err(e.into());
                        }
                    }
                }
            }
        };

        match res {
            Err(c) if c.should_kill_worker() => {
                shutdown.cancel();
                Err(c)
            }
            o => o,
        }
    }

    async fn send_message(&mut self, msg: ServiceMessage) -> Result {
        let msg = format!("{}\n", msg.serialize()?);
        self.stdin
            .write_all(msg.as_bytes())
            .await
            .context("Couldn't write message to worker")
    }

    async fn wait_for_new_message(&mut self, timeout: Option<Duration>) -> Result<WorkerMessage> {
        let mut buf = String::new();
        let shutdown_rx = self.shutdown.clone();

        let timeout = timeout.unwrap_or(self.timeout_internal);

        let res = Self::wait_for(self.stdout.read_line(&mut buf), shutdown_rx, timeout).await;

        match res {
            WaitForResult::Ok(res) => {
                res.context("Couldn't read worker message")?;
                let msg =
                    serde_json::from_str(&buf).context("Couldn't deserialize worker message")?;
                if let WorkerMessage::InternalError(why) = msg {
                    self.wait_child().await?;
                    bail!("Worker internal error: {}", why);
                } else if msg.is_internal() {
                    bail!("Unexpected internal message: {:?}", msg);
                } else {
                    Ok(msg)
                }
            }
            WaitForResult::Cancelled => Ok(WorkerMessage::Cancelled),
            WaitForResult::HardTimeout => Ok(WorkerMessage::TimedOut),
        }
    }

    async fn wait_child(&mut self) -> Result {
        let shutdown_rx = self.shutdown.clone();
        let res = Self::wait_for(self.child.wait(), shutdown_rx, self.timeout_internal).await;
        match res {
            WaitForResult::Ok(status) => {
                status
                    .map(|_| ())
                    .context("Failed to wait for worker status")?;
            }
            _ => {
                self.kill_child().await?;
            }
        }
        Ok(())
    }

    async fn wait_for<T>(
        future: impl Future<Output = T>,
        shutdown: CancellationToken,
        timeout: Duration,
    ) -> WaitForResult<T> {
        select! {
            res = future => WaitForResult::Ok(res),
            _ = shutdown.cancelled() => WaitForResult::Cancelled,
            _ = tokio::time::sleep(timeout), if timeout.as_secs() != 0 => WaitForResult::HardTimeout,
        }
    }

    async fn update_stats(&mut self) -> Result<CGroupStats> {
        let new_stat = self.cgroup.get_stats().await?;
        let diff = new_stat - self.last_stat;
        self.last_stat = new_stat;
        Ok(diff)
    }

    async fn check_stat_diff(
        diff: CGroupStats,
        cgroup: &CGroup,
        cpu_limit_seconds: u64,
    ) -> CaseResult {
        if diff.check_broke_cpu_time(cpu_limit_seconds * 1_000_000) {
            Err(CaseError::CpuTimeExceeded(diff.cpu_usage_usec))
        } else if diff.check_broke_memory_limit() {
            let usage = cgroup.get_memory_peak().await?;
            Err(CaseError::MemoryLimitExceeded(usage))
        } else {
            Ok(())
        }
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
        self.child
            .wait()
            .await
            .context("Couldn't wait for worker process")?;
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
        self.cgroup
            .shutdown_sync()
            .context("Couldn't shutdown cgroup")
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
