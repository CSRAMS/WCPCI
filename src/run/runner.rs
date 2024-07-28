use std::process::Stdio;

use log::{error, info};
use tokio::io::AsyncWriteExt;
use tokio::{io::AsyncReadExt, select};

use crate::problems::TestCase;

use super::languages::CommandInfo;
use super::{job::CaseStatus, manager::ShutdownReceiver};

#[derive(Debug, Clone)]
pub enum CaseError {
    Logic,
    //TimeLimitExceeded,
    Runtime(String),
    Compilation(String),
    Judge(String),
    Cancelled,
}

impl From<CaseError> for CaseStatus {
    fn from(val: CaseError) -> Self {
        let status = match val {
            CaseError::Logic => "Logic error".to_string(),
            //CaseError::TimeLimitExceeded => "Time limit exceeded".to_string(),
            CaseError::Runtime(_) => "Runtime error".to_string(),
            CaseError::Compilation(_) => "Compile error".to_string(),
            CaseError::Judge(_) => "Judge error".to_string(),
            CaseError::Cancelled => "Run Cancelled".to_string(),
        };
        let penalty = matches!(val, CaseError::Logic | CaseError::Runtime(_));
        CaseStatus::Failed(penalty, status)
    }
}

pub type CaseResult<T = ()> = Result<T, CaseError>;

pub struct Runner {
    run_cmd: CommandInfo,
    compile_cmd: Option<CommandInfo>,
    shutdown_rx: ShutdownReceiver,
    #[allow(dead_code)]
    max_cpu_time: i64,
}

impl Runner {
    pub async fn new(
        compile_cmd: &Option<CommandInfo>,
        run_cmd: &CommandInfo,
        file_name: &str,
        program: &str,
        max_cpu_time: i64,
        shutdown_rx: ShutdownReceiver,
    ) -> CaseResult<Self> {
        tokio::fs::write(file_name, program.as_bytes())
            .await
            .map_err(|e| CaseError::Judge(format!("Couldn't write to program file: {e:?}")))?;

        Ok(Self {
            run_cmd: run_cmd.clone(),
            compile_cmd: compile_cmd.clone(),
            max_cpu_time,
            shutdown_rx,
        })
    }

    pub async fn compile(&mut self) -> Result<(), CaseError> {
        if let Some(ref compile_cmd) = self.compile_cmd {
            let mut cmd = compile_cmd.make_command();
            cmd.stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .kill_on_drop(true);

            debug!("Running command: {:?}", cmd);

            let output = cmd
                .output()
                .await
                .map_err(|e| CaseError::Judge(format!("Couldn't run compile command: {e:?}")))?;
            if !output.status.success() {
                let std_err = String::from_utf8_lossy(&output.stderr).to_string();
                Err(CaseError::Compilation(std_err))
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    pub async fn run_cmd(&self, input: &str) -> CaseResult<String> {
        let mut cmd = self.run_cmd.make_command();

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);

        // const AA: &str = "/nix/store";
        // debug!("List: {:?}", std::fs::read_dir(AA).unwrap().collect::<Vec<_>>());
        // const AAAA: &str = "main.py";
        // debug!("Main Exists?: {:?}", PathBuf::from(AAAA).exists());
        // debug!("PATH is {:?}", std::env::var("PATH").unwrap_or_default());
        // debug!("Current dir is {:?}", std::env::current_dir().unwrap());

        let mut child = cmd
            .spawn()
            .map_err(|e| CaseError::Judge(format!("Couldn't spawn process: {e:?}")))?;

        let stdin = child
            .stdin
            .as_mut()
            .ok_or(CaseError::Judge("Couldn't open stdin".to_string()))?;

        stdin
            .write_all(input.as_bytes())
            .await
            .map_err(|e| CaseError::Judge(format!("Couldn't write to stdin: {e:?}")))?;

        let mut shutdown_rx = self.shutdown_rx.clone();

        let res = select! {
            res = child.wait() => {
                res.map_err(|e| CaseError::Judge(format!("Couldn't wait for process: {e:?}")))?
            }
            _ = shutdown_rx.changed() => {
                child.kill().await.map_err(|e| CaseError::Judge(format!("Couldn't kill process: {e:?}")))?;
                info!("Process killed forcefully");
                Err(CaseError::Cancelled)?
            }
        };

        // Sleep for a bit for pizzaz
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;

        if res.success() {
            let mut stdout = child
                .stdout
                .ok_or(CaseError::Judge("Couldn't open stdout".to_string()))?;
            let mut output = String::new();
            stdout
                .read_to_string(&mut output)
                .await
                .map_err(|e| CaseError::Judge(format!("Couldn't read stdout: {e:?}")))?;
            Ok(output)
        } else {
            let mut stderr = child
                .stderr
                .ok_or(CaseError::Judge("Couldn't open stderr".to_string()))?;
            let mut std_err = String::new();
            stderr
                .read_to_string(&mut std_err)
                .await
                .map_err(|e| CaseError::Judge(format!("Couldn't read stderr: {e:?}")))?;
            let code = res.code().unwrap_or(-1);
            error!("Process exited with error {code}:\n\n {std_err}");
            Err(CaseError::Runtime(format!(
                "Process exited with error {code}:\n\n {std_err}"
            )))
        }
    }

    pub async fn run_case(&self, case: &TestCase) -> CaseResult<String> {
        let output = self.run_cmd(&case.stdin).await?;

        let res = case.check_output(&output, &case.expected_pattern);
        res.map_err(CaseError::Judge).and_then(
            |b| {
                if b {
                    Ok(output)
                } else {
                    Err(CaseError::Logic)
                }
            },
        )
    }
}
