use std::sync::Arc;

use log::{error, warn};
use rocket::{fairing::AdHoc, routes};
use rocket_db_pools::Database as R_Database;
use tokio::sync::Mutex;

use crate::{db::Database, leaderboard::LeaderboardManagerHandle};

use self::manager::RunManager;

mod job;
mod languages;
mod manager;
mod runner;
mod ws;

pub type JobStateMessage = job::JobState;

pub type JobStateSender = tokio::sync::watch::Sender<JobStateMessage>;
pub type JobStateReceiver = tokio::sync::watch::Receiver<JobStateMessage>;

pub type ManagerHandle = Arc<Mutex<RunManager>>;

pub use job::JobState;
pub use languages::RunConfig;

pub struct CodeInfo {
    pub run_config: RunConfig,
    pub languages_json: String,
}

pub fn stage() -> AdHoc {
    let (tx, rx) = tokio::sync::watch::channel(false);

    AdHoc::try_on_ignite("Runner App", |rocket| async {
        let pool = match Database::fetch(&rocket) {
            Some(pool) => pool.0.clone(), // clone the wrapped pool
            None => return Err(rocket),
        };

        let shutdown_fairing = AdHoc::on_shutdown("Shutdown Runners / Sockets", |rocket| {
            Box::pin(async move {
                tx.send(true).ok();
                if let Some(manager) = rocket.state::<ManagerHandle>() {
                    manager.lock().await.shutdown().await;
                }
            })
        });

        let config = rocket.figment().extract_inner::<RunConfig>("run");

        match config {
            Err(e) => {
                error!("Couldn't load run config: {:?}", e);
                Err(rocket)
            }
            Ok(mut config) => {
                if !config.languages.contains_key(&config.default_language) {
                    if let Some((k, _)) = config.languages.iter().next() {
                        warn!(
                            "Default language not in 'run.languages', using first language: {}",
                            k
                        );
                        config.default_language.clone_from(k);
                    } else {
                        error!("No languages found in config key 'run.languages'");
                        return Err(rocket);
                    }
                };
                let code_info = serde_json::to_string(&config.languages).unwrap();
                let leaderboard_manager =
                    rocket.state::<LeaderboardManagerHandle>().unwrap().clone();
                let manager =
                    manager::RunManager::new(config.clone(), leaderboard_manager, pool, rx);
                Ok(rocket
                    .attach(shutdown_fairing)
                    .manage::<CodeInfo>(CodeInfo {
                        run_config: config,
                        languages_json: code_info,
                    })
                    .manage::<ManagerHandle>(Arc::new(Mutex::new(manager)))
                    .mount("/run", routes![ws::ws_channel]))
            }
        }
    })
}
