use std::{collections::HashMap, sync::Arc};

use log::info;
use rand::{distr::Alphanumeric, Rng};
use rocket::{
    fairing::AdHoc,
    futures::lock::Mutex,
    http::{Cookie, CookieJar, SameSite, Status},
    outcome::IntoOutcome,
    request::{self, FromRequest},
    time::{Duration, OffsetDateTime},
    Request, State,
};
use serde::Serialize;

use crate::{auth::sessions::Session, error::prelude::*};

#[derive(Debug)]
pub struct CsrfTokens {
    tokens: HashMap<CsrfToken, (Option<String>, OffsetDateTime)>,
}

type ArCsrfTokens = Arc<Mutex<CsrfTokens>>;

impl CsrfTokens {
    const LENGTH_FOR_PRUNE: usize = 100;

    pub fn new() -> Self {
        Self {
            tokens: HashMap::with_capacity(10),
        }
    }

    pub fn generate(&mut self, session_token: Option<&str>) -> CsrfToken {
        let token = CsrfToken::generate();
        let now = OffsetDateTime::now_utc();
        if let Some(session_token) = session_token {
            self.prune_session(session_token);
        }
        if self.tokens.len() > Self::LENGTH_FOR_PRUNE {
            info!("Pruning CSRF tokens");
            self.prune();
        }
        self.tokens
            .insert(token.clone(), (session_token.map(|s| s.to_string()), now));
        token
    }

    fn prune_session(&mut self, session_token: &str) {
        self.tokens
            .retain(|_, (token_session, _)| token_session.as_deref() != Some(session_token));
    }

    fn prune(&mut self) {
        let now = OffsetDateTime::now_utc();
        self.tokens.retain(|_, (_, time_set)| {
            now < *time_set + Duration::minutes(CsrfToken::TOKEN_COOKIE_LIFETIME_MINUTES)
        });
    }

    pub fn validate(
        &mut self,
        session_token: Option<&str>,
        token: &CsrfToken,
    ) -> Option<CsrfToken> {
        if let Some(inner) = self.tokens.get(token).cloned() {
            if let (Some(received_session_token), time_set) = inner {
                if let Some(expected_session_token) = session_token {
                    let now = OffsetDateTime::now_utc();
                    let expired = now
                        > time_set + Duration::minutes(CsrfToken::TOKEN_COOKIE_LIFETIME_MINUTES);
                    let matches = expected_session_token == received_session_token;
                    if expired || matches {
                        self.tokens.remove(token); // It's either expired or it's matching, so clear the entry either way
                        if !expired && matches {
                            Some(self.generate(session_token)) // It's matching and not expired
                        } else {
                            None // It's expired or doesn't match
                        }
                    } else {
                        None // It's not expired and doesn't match
                    }
                } else {
                    None // User has no session token but csrf token does
                }
            } else {
                Some(self.generate(session_token)) // True for anonymous users
            }
        } else {
            None // No such token
        }
    }
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub struct CsrfToken(String);

impl CsrfToken {
    const TOKEN_LENGTH: usize = 32;
    pub const TOKEN_COOKIE_NAME: &'static str = "csrf_token";
    pub const TOKEN_COOKIE_LIFETIME_MINUTES: i64 = 60;

    pub fn generate() -> Self {
        let rng = rand::rng();
        Self(
            rng.sample_iter(&Alphanumeric)
                .take(Self::TOKEN_LENGTH)
                .map(char::from)
                .collect(),
        )
    }
}

impl Serialize for CsrfToken {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for &'r CsrfToken {
    type Error = &'r anyhow::Error;

    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let token_result: &Result<CsrfToken> = req
            .local_cache_async(async {
                let cookies = req
                    .guard::<&CookieJar<'_>>()
                    .await
                    .success_or_else(|| anyhow!("Couldn't get cookies"))?;
                let tokens = req
                    .guard::<&State<ArCsrfTokens>>()
                    .await
                    .success_or_else(|| anyhow!("Couldn't get tokens"))?;
                let session_token = cookies
                    .get_private(Session::TOKEN_COOKIE_NAME)
                    .map(|c| c.value().to_string());

                let mut tokens = tokens.lock().await;
                let token = tokens.generate(session_token.as_deref());
                cookies.add_private(
                    Cookie::build((CsrfToken::TOKEN_COOKIE_NAME, token.0.clone()))
                        .same_site(SameSite::Strict)
                        .max_age(Duration::minutes(CsrfToken::TOKEN_COOKIE_LIFETIME_MINUTES)),
                );
                Ok(token)
            })
            .await;

        token_result
            .as_ref()
            .or_forward(Status::InternalServerError)
    }
}

pub struct VerifyCsrfToken();

#[rocket::async_trait]
impl<'r> FromRequest<'r> for &'r VerifyCsrfToken {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> request::Outcome<Self, Self::Error> {
        let token_result = req
            .local_cache_async(async {
                let cookies = req.guard::<&CookieJar<'_>>().await.succeeded()?;
                let tokens = req.guard::<&State<ArCsrfTokens>>().await.succeeded()?;
                let session_token = cookies
                    .get_private(Session::TOKEN_COOKIE_NAME)
                    .map(|c| c.value().to_string());
                let csrf_token = cookies
                    .get_private(CsrfToken::TOKEN_COOKIE_NAME)
                    .map(|c| c.value().to_string());

                if let Some(csrf_token) = csrf_token {
                    let mut tokens = tokens.lock().await;
                    if let Some(new_token) =
                        tokens.validate(session_token.as_deref(), &CsrfToken(csrf_token))
                    {
                        cookies.add_private(
                            Cookie::build((CsrfToken::TOKEN_COOKIE_NAME, new_token.0.clone()))
                                .same_site(SameSite::Strict)
                                .max_age(Duration::minutes(
                                    CsrfToken::TOKEN_COOKIE_LIFETIME_MINUTES,
                                )),
                        );
                        Some(VerifyCsrfToken())
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .await;

        token_result.as_ref().or_forward(Status::Forbidden)
    }
}

pub fn stage() -> AdHoc {
    AdHoc::on_ignite("CSRF App", |rocket| async {
        rocket.manage(Arc::new(Mutex::new(CsrfTokens::new())))
    })
}
