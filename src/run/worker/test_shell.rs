use std::io::Write;

use tokio::{io::AsyncBufReadExt, select};

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
    eprintln!("Starting test shell");
    let conf: RunConfig = rocket::config::Config::figment()
        .extract_inner::<RunConfig>("run")
        .context("Couldn't get run config")?;

    let res = start(&conf).await;

    if let Err(e) = res {
        eprintln!("Worker Error: {:?}", e);
    }

    std::process::exit(0);
}

async fn start(conf: &RunConfig) -> Result {
    let (_shutdown_tx, shutdown_rx) = tokio::sync::watch::channel::<bool>(false);

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

    iso.setup().context("Couldn't setup isolation")?;

    let mut worker = Worker::new(0, "", shutdown_rx, debug_run_info, iso, "Test Shell")
        .await
        .context("Worker Creation Failed")?;

    let stdin = tokio::io::stdin();
    let mut stdin = tokio::io::BufReader::new(stdin);
    let mut buf = String::new();

    eprintln!("Basic shell started. Type :exit to exit");
    eprintln!("The underlying shell inside the container is bash");
    eprintln!("Anything you pass will be executed as a bash command");
    eprintln!("Keep in mind this has no line reader, so you can't use arrow keys");
    eprintln!(
        "Also, environments aren't kept between commands so stuff like `cd` won't have effect"
    );
    eprintln!("Your PATH has been forwarded to the container");

    let ctrl_c = tokio::signal::ctrl_c();

    tokio::pin!(ctrl_c);

    loop {
        eprint!("> ");
        std::io::stdout().flush().context("Couldn't flush stdout")?;

        select! {
            res = &mut ctrl_c => {
                res.context("Couldn't get ctrl-c")?;
                eprintln!();
                break;
            }
            res = stdin.read_line(&mut buf) => {
                let read = res.context("Couldn't read from stdin")?;

                let instant = std::time::Instant::now();

                if read == 0 || buf.trim() == ":exit" {
                    eprintln!();
                    break;
                } else {
                    let res = worker.run_cmd(Some(&buf)).await;
                    print_output(res);
                }

                eprintln!("%% Time taken: {:?} %%", instant.elapsed());

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
        Err(other) => {
            println!("!! You shouldn't be getting this !!");
            println!("!! {other:?} !!");
        }
    }
}
