use rocket::figment::providers::{Env, Format, Toml};
use rocket::figment::{Figment, Profile};
use rocket::{get, routes, Build, Config};
use rocket_dyn_templates::Template;

#[macro_use]
extern crate rocket_dyn_templates;

#[macro_use]
extern crate serde;

#[macro_use]
extern crate rocket;

mod admin;
mod auth;
mod branding;
mod contests;
mod csp;
mod db;
mod error;
mod leaderboard;
mod messages;
mod problems;
mod profile;
mod run;
mod serve;
mod settings;
#[macro_use]
mod template;
mod times;

use crate::auth::users::User;
use crate::error::prelude::*;

#[get("/")]
async fn index(user: Option<&User>) -> Template {
    let ctx = context_with_base!(user,);
    Template::render("index", ctx)
}

#[get("/md-help")]
async fn md_help(user: Option<&User>) -> Template {
    let ctx = context_with_base!(user,);
    Template::render("md_help", ctx)
}

pub fn figment() -> Result<Figment> {
    let config_path = Env::var("OXIDEJUDGE_CONFIG").context("OXIDEJUDGE_CONFIG was not set")?;
    let secrets_path = Env::var("OXIDEJUDGE_SECRETS").context("OXIDEJUDGE_SECRETS was not set")?;
    let figment = Figment::from(Config::default())
        .merge(Toml::file(config_path))
        .merge(Toml::file(secrets_path))
        // TODO(Spoon): set `ident`? set oauth.$1.provider = $1? set cli_colors = false by default
        .merge(
            Env::prefixed("OXIDEJUDGE_") // TODO: just stuff for DB URL, template dir, saml certs (& TLS certs?)
                .ignore(&["CONFIG", "SECRETS", "PROFILE"])
                .global(),
        )
        .select(Profile::from_env_or(
            "OXIDEJUDGE_PROFILE",
            Config::DEFAULT_PROFILE,
        ));
    Ok(figment)
}

fn rocket(figment: Figment) -> rocket::Rocket<Build> {
    println!("Start of WCPC v{}", env!("CARGO_PKG_VERSION"));

    rocket::custom(figment)
        .mount("/", routes![index, md_help])
        .attach(error::stage())
        .attach(db::stage())
        .attach(times::stage())
        .attach(template::stage())
        .attach(serve::stage())
        .attach(branding::stage())
        .attach(auth::stage())
        .attach(settings::stage())
        .attach(admin::stage())
        .attach(contests::stage())
        .attach(problems::stage())
        .attach(leaderboard::stage())
        .attach(profile::stage())
}

// It's the main function so I'm not really concerned with sizes
#[allow(clippy::result_large_err)]
#[rocket::main]
async fn _main() -> Result<()> {
    let figment = figment()?;
    rocket(figment).ignite().await?.launch().await?;
    Ok(())
}

fn main() -> Result {
    let args = std::env::args().collect::<Vec<_>>();

    if args.contains(&"--worker".to_string()) {
        run::worker::run_from_child();
        Ok(())
    } else if args.contains(&"--worker-test-shell".to_string()) {
        run::worker::run_test_shell().context("Worker test shell failed")
    } else {
        _main().context("Rocket failed")
    }
}
