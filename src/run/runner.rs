use std::io::{Read, Write};
use std::process::Stdio;

use log::error;

use crate::problems::TestCase;

use super::job::CaseStatus;
use super::languages::CommandInfo;

#[derive(Debug, Clone)]
pub enum CaseError {
    Logic,
    //TimeLimitExceeded,
    Runtime(String),
    Compilation(String),
    Judge(String),
}

impl From<CaseError> for CaseStatus {
    fn from(val: CaseError) -> Self {
        let status = match val {
            CaseError::Logic => "Logic error".to_string(),
            //CaseError::TimeLimitExceeded => "Time limit exceeded".to_string(),
            CaseError::Runtime(_) => "Runtime error".to_string(),
            CaseError::Compilation(_) => "Compile error".to_string(),
            CaseError::Judge(_) => "Judge error".to_string(),
        };
        let penalty = matches!(val, CaseError::Logic | CaseError::Runtime(_));
        CaseStatus::Failed(penalty, status)
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
            Ok(output)
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

    pub fn run_case(&self, case: &TestCase) -> CaseResult<String> {
        let output = self.run_cmd(&case.stdin)?;

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
