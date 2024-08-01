use std::{
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

use log::{Metadata, Record};

use crate::{error::prelude::*, wait_for_msg};

use super::{ServiceMessage, WorkerMessage};

pub fn run_from_child() {
    WorkerLogger::setup();
    info!("Starting Worker...");
    let cwd = std::env::current_dir().unwrap();
    if let Err(e) = _run_from_child(&cwd) {
        WorkerMessage::InternalError(format!("{e:?}"))
            .send()
            .unwrap();
    }
}

fn _run_from_child(dir: &Path) -> Result {
    let init = wait_for_msg!(ServiceMessage::InitialInfo(i) => i)?;

    info!("{}", init.diagnostic_info);

    super::super::isolation::isolate(&init.isolation_config, dir)
        .context("Couldn't isolate process")?;

    std::fs::write(&init.file_name, &init.program).context("Couldn't write program to file")?;

    info!("Worker Started");

    WorkerMessage::Ready.send()?;

    loop {
        match ServiceMessage::wait_for()? {
            ServiceMessage::RunCmd(cmd, stdin, env) => {
                let mut cmd = cmd.make_command();
                cmd.envs(env).stdin(if stdin.is_some() {
                    Stdio::piped()
                } else {
                    Stdio::null()
                });
                run_cmd(cmd, stdin)?;
            }
            ServiceMessage::Stop => {
                info!("Stopping Worker");
                break;
            }
            _ => {
                warn!("Invalid message from service");
            }
        }
    }

    Ok(())
}

fn run_cmd(mut cmd: Command, stdin: Option<String>) -> Result {
    let mut child = cmd.spawn().context("Couldn't spawn process")?;
    if let Some(stdin_s) = stdin {
        let stdin = child.stdin.as_mut().context("Couldn't open stdin")?;
        stdin
            .write_all(stdin_s.as_bytes())
            .context("Couldn't write to stdin")?;
    }
    let output = child
        .wait_with_output()
        .context("Couldn't wait for process")?;
    WorkerMessage::CmdComplete(output.into()).send()
}

struct WorkerLogger(String);

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
