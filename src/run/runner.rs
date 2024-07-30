use std::io::{Read, Write};
use std::process::Stdio;

use log::error;

use super::job::{CaseStatus, JobFailure};
use super::languages::CommandInfo;
use super::WorkerMessage;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "err", content = "data", rename_all = "camelCase")]
pub enum CaseError {
    Logic,
    //TimeLimitExceeded,
    Runtime(String),
    Compilation(String),
    Judge(String),
}

impl From<CaseError> for JobFailure {
    fn from(val: CaseError) -> Self {
        Self::Initial(val)
    }
}

impl CaseError {
    pub fn to_string(&self, details: bool) -> String {
        match self {
            CaseError::Logic => "Logic Error".to_string(),
            CaseError::Runtime(ref s) => {
                if details {
                    format!("Runtime Error:\n{s}")
                } else {
                    "Runtime Error".to_string()
                }
            }
            CaseError::Compilation(ref s) => {
                if details {
                    format!("Compilation Error:\n{s}")
                } else {
                    "Compilation Error".to_string()
                }
            }
            CaseError::Judge(_) => "Judge Error".to_string(),
        }
    }
}

impl From<CaseError> for CaseStatus {
    fn from(val: CaseError) -> Self {
        let penalty = matches!(val, CaseError::Logic | CaseError::Runtime(_));
        CaseStatus::Failed(penalty, val.into())
    }
}

pub type CaseResult<T = ()> = Result<T, CaseError>;

pub struct Runner {
    run_cmd: CommandInfo,
    compile_cmd: Option<CommandInfo>,
    #[allow(dead_code)]
    max_cpu_time: i64,
}

impl Runner {
    pub fn new(
        compile_cmd: &Option<CommandInfo>,
        run_cmd: &CommandInfo,
        file_name: &str,
        program: &str,
        max_cpu_time: i64,
    ) -> CaseResult<Self> {
        std::fs::write(file_name, program.as_bytes())
            .map_err(|e| CaseError::Judge(format!("Couldn't write to program file: {e:?}")))?;

        Ok(Self {
            run_cmd: run_cmd.clone(),
            compile_cmd: compile_cmd.clone(),
            max_cpu_time,
        })
    }

    pub fn compile(&mut self) -> Result<(), CaseError> {
        if let Some(ref compile_cmd) = self.compile_cmd {
            let mut cmd = compile_cmd.make_command();
            cmd.stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            debug!("Running command: {:?}", cmd);

            let output = cmd
                .output()
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

    pub fn run_cmd(&self, input: &str) -> CaseResult<String> {
        let mut cmd = self.run_cmd.make_command();

        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = cmd
            .spawn()
            .map_err(|e| CaseError::Judge(format!("Couldn't spawn process: {e:?}")))?;

        let stdin = child
            .stdin
            .as_mut()
            .ok_or(CaseError::Judge("Couldn't open stdin".to_string()))?;

        stdin
            .write_all(input.as_bytes())
            .map_err(|e| CaseError::Judge(format!("Couldn't write to stdin: {e:?}")))?;

        let res = child
            .wait()
            .map_err(|e| CaseError::Judge(format!("Couldn't wait for process: {e:?}")))?;

        // Sleep for a bit for pizzaz
        std::thread::sleep(std::time::Duration::from_millis(500));

        if res.success() {
            let mut stdout = child
                .stdout
                .ok_or(CaseError::Judge("Couldn't open stdout".to_string()))?;
            let mut output = String::new();
            stdout
                .read_to_string(&mut output)
                .map_err(|e| CaseError::Judge(format!("Couldn't read stdout: {e:?}")))?;

            let msg = WorkerMessage::RequestCheck(output.clone());
            let msg = serde_json::to_string(&msg)
                .map_err(|e| CaseError::Judge(format!("Couldn't serialize message: {e:?}")))?;
            println!("{}", msg);

            debug!("Awaiting parent check");

            let stdin = std::io::stdin();
            let mut buf = String::new();
            stdin
                .read_line(&mut buf)
                .map_err(|e| CaseError::Judge(format!("Couldn't read from stdin: {e:?}")))?;

            debug!("Parent check: {:?}", buf);

            if buf.trim().to_lowercase() == "y" {
                Ok(output)
            } else {
                Err(CaseError::Logic)
            }
        } else {
            let mut stderr = child
                .stderr
                .ok_or(CaseError::Judge("Couldn't open stderr".to_string()))?;
            let mut std_err = String::new();
            stderr
                .read_to_string(&mut std_err)
                .map_err(|e| CaseError::Judge(format!("Couldn't read stderr: {e:?}")))?;
            let code = res.code().unwrap_or(-1);
            error!("Process exited with error {code}:\n\n {std_err}");
            Err(CaseError::Runtime(format!(
                "Process exited with error {code}:\n\n {std_err}"
            )))
        }
    }
}
