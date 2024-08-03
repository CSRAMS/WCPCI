use std::{collections::HashMap, fmt::Display, os::unix::process::ExitStatusExt, process::Output};

use crate::error::prelude::*;

use super::config::CommandInfo;

mod isolation;
/// Service process side of the worker
mod service_side;
mod test_shell;
/// Worker process side of the worker
mod worker_side;

pub use isolation::IsolationConfig;
use nix::sys::signal::Signal;
pub use service_side::Worker;
pub use test_shell::run_test_shell;
pub use worker_side::run_from_child;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InitialWorkerInfo {
    pub diagnostic_info: String,
    pub isolation_config: isolation::IsolationConfig,
    pub program: String,
    pub file_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Message from the service process to the worker process.
pub enum ServiceMessage {
    /// Gives initial information to the worker process.
    /// diagnostic_info, isolation_config, program
    InitialInfo(InitialWorkerInfo),
    /// Run a command inside the worker process.
    /// command, stdin, env vars
    RunCmd(CommandInfo, Option<String>, HashMap<String, String>),
    /// Confirm to the worker that it's UID and GID maps have been set
    /// status (true if successful)
    UidGidMapResult(bool),
    /// Stop the worker process.
    Stop,
}

impl ServiceMessage {
    pub fn wait_for() -> Result<Self> {
        let mut buf = String::new();
        std::io::stdin()
            .read_line(&mut buf)
            .context("Couldn't read message from service")?;
        serde_json::from_str(&buf).context("Couldn't deserialize message from service")
    }

    pub fn serialize(&self) -> Result<String> {
        serde_json::to_string(self).context("Couldn't serialize message")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CmdOutput {
    stdout: String,
    stderr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CmdExit {
    status: Option<i32>,
    signal: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CmdFailure(CmdOutput, CmdExit);

impl CmdFailure {
    fn interpret_exit_status(&self) -> String {
        let ex = self
            .1
            .status
            .map(|code| format!("with exit code {code}"))
            .or_else(|| {
                self.1.signal.map(|signo| {
                    if let Some(signame) = Signal::try_from(signo).ok().map(|s| s.as_str()) {
                        format!("with signal {signame} ({signo})")
                    } else {
                        format!("with signal {signo}")
                    }
                })
            })
            .unwrap_or_else(|| "unexpectedly".to_string());
        format!("Process exited {ex}")
    }

    fn stdout_stderr(&self) -> String {
        let stderr = Some(self.0.stderr.trim())
            .filter(|s| !s.is_empty())
            .map(|s| format!("\n{s}"));
        format!("{}{}", self.0.stdout, stderr.unwrap_or_default())
    }
}

impl Display for CmdFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}\n\n{}",
            self.interpret_exit_status(),
            self.stdout_stderr()
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum CmdResult {
    Success(CmdOutput),
    Failure(CmdFailure),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
/// Message from the worker process to the service process.
pub enum WorkerMessage {
    /// A completed command with its output.
    CmdComplete(CmdResult),
    /// Request service to create a UID and GID mapping.
    /// Contains the PID of the worker process post-fork.
    RequestUidGidMap(i32),
    /// Internal failure in some way, sign to stop the worker.
    InternalError(String),
    /// Service process is ready to receive commands.
    Ready,
    /// Not technically from the worker, used signify when a wait for message was cancelled
    Cancelled,
}

impl WorkerMessage {
    pub fn send(&self) -> Result {
        let msg = serde_json::to_string(self).context("Couldn't serialize message")?;
        println!("{}", msg);
        Ok(())
    }
}

impl From<Output> for CmdResult {
    fn from(output: Output) -> Self {
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        if output.status.success() {
            Self::Success(CmdOutput { stdout, stderr })
        } else {
            Self::Failure(CmdFailure(
                CmdOutput { stdout, stderr },
                CmdExit {
                    status: output.status.code(),
                    signal: output.status.signal(),
                },
            ))
        }
    }
}

#[macro_export]
macro_rules! wait_for_msg {
    ($pat:pat => $body:expr) => {
        $crate::run::worker::ServiceMessage::wait_for().and_then(|msg| match msg {
            $pat => Ok($body),
            _ => Err(anyhow!("Unexpected message: {msg:?}")),
        })
    };
}

pub type CaseResult<T = ()> = Result<T, CaseError>;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "err", content = "data", rename_all = "camelCase")]
pub enum CaseError {
    Logic,
    //TimeLimitExceeded,
    Runtime(String),
    Compilation(String),
    Judge(String),
    Cancelled,
}

impl From<anyhow::Error> for CaseError {
    fn from(e: anyhow::Error) -> Self {
        CaseError::Judge(format!("{e:?}"))
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
            CaseError::Cancelled => "Run Cancelled".to_string(),
        }
    }
}
