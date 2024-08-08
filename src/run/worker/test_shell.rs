use std::io::Write;

use tokio::{io::AsyncBufReadExt, select};
use tokio_util::sync::CancellationToken;

use crate::{
    error::prelude::*,
    run::{
        config::{CommandInfo, LanguageRunnerInfo},
        worker::Worker,
        RunConfig,
    },
};

use super::{CaseError, CaseResult};

#[tokio::main]
pub async fn run_test_shell() -> Result {
    TestShellLogger::setup();
    info!("Starting test shell");
    let conf: RunConfig = rocket::config::Config::figment()
        .extract_inner::<RunConfig>("run")
        .context("Couldn't get run config")?;

    let res = start(&conf).await;

    if let Err(e) = res {
        error!("Worker Error: {:?}", e);
    }

    std::process::exit(0);
}

async fn start(conf: &RunConfig) -> Result {
    let shutdown = CancellationToken::new();

    let mut run_cmd_info = CommandInfo {
        binary: "bash".to_string(),
        args: vec![],
    };

    run_cmd_info.setup().context("Couldn't setup command")?;

    let path = std::env::var("PATH").context("Couldn't get PATH")?;

    let debug_run_info = LanguageRunnerInfo {
        file_name: ".dummy".to_string(),
        compile_cmd: None,
        run_cmd: run_cmd_info,
        env: [("PATH".to_string(), path)].into_iter().collect(),
    };

    let mut iso = conf.isolation.clone();

    iso.setup(false).await.context("Couldn't setup isolation")?;

    const DEBUG_SOFT_LIMITS: (u64, u64) = (5, 1024 * 1024 * 1024);

    let mut worker = Worker::new(
        0,
        "",
        shutdown,
        debug_run_info,
        iso,
        0,
        "Test Shell",
        DEBUG_SOFT_LIMITS,
    )
    .await
    .context("Worker Creation Failed")?;

    let stdin = tokio::io::stdin();
    let mut stdin = tokio::io::BufReader::new(stdin);
    let mut buf = String::new();

    info!("Basic shell started. Type :exit to exit");
    info!("The underlying shell inside the container is bash");
    info!("Anything you pass will be executed as a bash command");
    info!("Keep in mind this has no line reader, so you can't use arrow keys");
    info!("Also, environments aren't kept between commands so stuff like `cd` won't have effect");
    info!("Your PATH has been forwarded to the container");

    let ctrl_c = tokio::signal::ctrl_c();

    tokio::pin!(ctrl_c);

    loop {
        eprint!("> ");
        std::io::stdout().flush().context("Couldn't flush stdout")?;

        select! {
            res = &mut ctrl_c => {
                res.context("Couldn't get ctrl-c")?;
                break;
            }
            res = stdin.read_line(&mut buf) => {
                let read = res.context("Couldn't read from stdin")?;
                if read == 0 || buf.trim() == ":exit" {
                    break;
                } else {
                    let res = worker.run_cmd(Some(&buf)).await;
                    print_output(res);
                }
                buf.clear();
            }
        }
    }

    worker.finish().await?;

    Ok(())
}

fn print_output(res: CaseResult<String>) {
    match res {
        Ok(output) | Err(CaseError::Runtime(output)) => {
            println!("{}", output.trim_end());
        }
        Err(CaseError::Cancelled) => {
            println!("!! Worker got cancelled mid-run possibly due to timeout !!");
        }
        Err(CaseError::Judge(err)) => {
            println!("!! Judge Error: {err} !!");
        }
        Err(CaseError::HardTimeLimitExceeded) => {
            println!("!! Time Limit Exceeded !!");
        }
        Err(other) => {
            println!("!! You shouldn't be getting this !!");
            println!("!! {other:?} !!");
        }
    }
}

pub struct TestShellLogger;

impl TestShellLogger {
    pub fn setup() {
        let logger = Self;
        log::set_max_level(log::LevelFilter::Debug);
        let b = Box::new(logger);
        let res = log::set_boxed_logger(b);
        if res.is_err() {
            eprintln!("Failed to set logger: {:?}", res.err());
        }
    }
}

impl log::Log for TestShellLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Debug
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[Shell][{}]: {}", record.level(), record.args());
        }
    }

    fn flush(&self) {}
}
