use rocket::{fairing::AdHoc, http::Status, response::Redirect, Request};
use rocket_dyn_templates::Template;

#[derive(Responder, Debug)]
pub enum ResponseErr {
    Internal(rocket::response::Debug<anyhow::Error>),
    Status(Status),
}

#[derive(Responder)]
pub enum FormResponseFailure {
    Validation(Template),
    Error(ResponseErr),
}

pub type FormResponse = Result<Redirect, FormResponseFailure>;

impl From<anyhow::Error> for FormResponseFailure {
    fn from(e: anyhow::Error) -> Self {
        error!("Internal server error: {:?}", e);
        FormResponseFailure::Error(e.into())
    }
}

impl From<ResponseErr> for FormResponseFailure {
    fn from(err: ResponseErr) -> Self {
        FormResponseFailure::Error(err)
    }
}

impl From<Template> for FormResponseFailure {
    fn from(template: Template) -> Self {
        FormResponseFailure::Validation(template)
    }
}

impl From<Status> for FormResponseFailure {
    fn from(status: Status) -> Self {
        FormResponseFailure::Error(status.into())
    }
}

impl From<anyhow::Error> for ResponseErr {
    fn from(e: anyhow::Error) -> Self {
        error!("Internal server error: {:?}", e);
        ResponseErr::Internal(rocket::response::Debug(e))
    }
}

impl From<Status> for ResponseErr {
    fn from(s: Status) -> Self {
        ResponseErr::Status(s)
    }
}

pub mod prelude {
    pub use super::{FormResponse, ResponseErr};
    pub use anyhow::{anyhow, Context};
    use std::result::Result as StdResult;
    pub type Result<T = (), E = anyhow::Error> = StdResult<T, E>;
    pub type ResultResponse<T = ()> = StdResult<T, ResponseErr>;
}

#[catch(default)]
fn error_catcher(status: Status, _request: &Request) -> Template {
    let message = status.to_string();
    let code = status.code;
    Template::render(
        "error",
        context! { message, code, version: env!("CARGO_PKG_VERSION") },
    )
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("Error catcher", |rocket| async {
        rocket.register("/", catchers![error_catcher])
    })
}
