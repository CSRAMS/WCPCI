use log::error;
use rocket::{
    fairing::{self, AdHoc},
    Build, Rocket,
};
use rocket_db_pools::{Connection, Database as R_Database};
use sqlx::Sqlite;

#[derive(R_Database)]
#[database("sqlite_db")]
pub struct Database(pub sqlx::SqlitePool);

pub type DbPool = sqlx::SqlitePool;
pub type DbConnection = Connection<Database>;
pub type DbPoolConnection = sqlx::pool::PoolConnection<Sqlite>;

async fn run_migrations(rocket: Rocket<Build>) -> fairing::Result {
    match Database::fetch(&rocket) {
        Some(db) => match sqlx::migrate!("./migrations").run(&**db).await {
            Ok(_) => Ok(rocket),
            Err(e) => {
                error!("Failed to initialize SQLx database: {}", e);
                Err(rocket)
            }
        },
        None => Err(rocket),
    }
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("Database", |rocket| async {
        rocket
            .attach(Database::init())
            .attach(AdHoc::try_on_ignite("SQLx Migrations", run_migrations))
            .attach(super::run::stage()) // Needs to be here to ensure the database is initialized
    })
}
