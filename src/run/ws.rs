use log::error;
use rocket::{
    futures::{SinkExt, StreamExt},
    get,
    http::Status,
    State,
};
use rocket_ws::{stream::DuplexStream, WebSocket};
use serde::Deserialize;
use tokio::{
    select,
    time::{self, Duration, Instant},
};

use crate::{
    auth::users::{Admin, User},
    contests::Contest,
    db::DbConnection,
    error::prelude::*,
    problems::{Problem, TestCase},
    run::job::{JobOperation, JobRequest},
};

use super::{JobState, JobStateReceiver, ManagerHandle};

// Keep in sync with TypeScript type
#[derive(Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum WebSocketRequest {
    Judge {
        program: String,
        language: String,
    },
    Test {
        program: String,
        language: String,
        input: String,
    },
}

impl WebSocketRequest {
    pub fn program(&self) -> &str {
        match self {
            Self::Judge { program, .. } => program,
            Self::Test { program, .. } => program,
        }
    }

    pub fn language(&self) -> &str {
        match self {
            Self::Judge { language, .. } => language,
            Self::Test { language, .. } => language,
        }
    }
}

// Keep in sync with TypeScript type
#[derive(Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
enum WebSocketMessage {
    StateUpdate { state: JobState },
    RunStarted,
    RunDenied { reason: String },
    Invalid { error: String },
}

#[allow(clippy::large_enum_variant)]
enum LoopRes {
    Msg(WebSocketMessage),
    ChangeJobRx(JobStateReceiver),
    JobStart(JobRequest),
    Pong(Vec<u8>),
    Ping,
    Break,
    NoOp,
}

async fn websocket_loop(
    mut stream: DuplexStream,
    manager_handle: ManagerHandle,
    problem: Problem,
    test_cases: Vec<TestCase>,
    user_id: i64,
) {
    let mut manager = manager_handle.lock().await;
    let mut started_rx = manager.subscribe();
    let mut shutdown_rx = manager.subscribe_shutdown(&user_id).await;
    let mut updated_rx = manager.get_handle_for_problem(problem.id);
    let state_rx = manager.get_handle(user_id, problem.id).await;
    drop(manager);

    // Fake receiver to start the loop, will be replaced by the real one
    let (_, fake_rx) = tokio::sync::watch::channel(JobState::new_judging(0));

    let mut state_msg = None;

    let mut state_rx: JobStateReceiver = if let Some(rx) = state_rx {
        let r = rx.borrow();
        let msg = serde_json::to_string(&WebSocketMessage::StateUpdate { state: r.clone() })
            .map_err(|e| e.to_string())
            .unwrap();
        state_msg = Some(msg);
        drop(r);
        rx
    } else {
        fake_rx
    };

    if let Some(msg) = state_msg {
        let res = stream.send(rocket_ws::Message::Text(msg)).await;
        if let Err(e) = res {
            error!("Error sending message: {:?}", e);
        }
    }

    let sleep = time::sleep(Duration::from_secs(10));
    tokio::pin!(sleep);

    loop {
        let res = select! {
            () = &mut sleep => {
                sleep.as_mut().reset(Instant::now() + Duration::from_secs(10));
                LoopRes::Ping
            },
            Ok((user_id_incoming, problem_id, rx)) = started_rx.recv() => {
                if user_id_incoming == user_id && problem_id == problem.id {
                    LoopRes::ChangeJobRx(rx)
                } else {
                    LoopRes::NoOp
                }
            }
            client_message = stream.next() => {
                if let Some(client_message) = client_message {
                    if let Ok(client_message) = client_message {
                        match client_message {
                            rocket_ws::Message::Text(raw) => {
                                if let Ok(request) = serde_json::from_str::<WebSocketRequest>(&raw) {
                                    let op = match &request {
                                        WebSocketRequest::Judge { .. } => JobOperation::Judging(test_cases.clone()),
                                        WebSocketRequest::Test { input, .. } => JobOperation::Testing(input.to_string())
                                    };

                                    let mut manager = manager_handle.lock().await;

                                    if let Some(language) = manager.get_language_config(request.language()) {

                                        let id = manager.get_request_id();

                                        let job_to_start = JobRequest {
                                            id,
                                            user_id,
                                            problem_id: problem.id,
                                            contest_id: problem.contest_id,
                                            program: request.program().to_string(),
                                            language_key: request.language().to_string(),
                                            language,
                                            cpu_time: problem.cpu_time,
                                            op
                                        };
                                        LoopRes::JobStart(job_to_start)
                                    } else {
                                        LoopRes::Msg(WebSocketMessage::Invalid { error: "Invalid language".to_string() })
                                    }
                                } else {
                                    LoopRes::Msg(WebSocketMessage::Invalid { error: "Invalid request".to_string() })
                                }
                            },
                            rocket_ws::Message::Ping(e) => {
                                LoopRes::Pong(e)
                            },
                            rocket_ws::Message::Close(_) => {
                                LoopRes::Break
                            },
                            _ => {
                                LoopRes::NoOp
                            }
                        }
                    } else {
                        LoopRes::NoOp
                    }
                } else {
                    LoopRes::Break
                }
            }
            Ok(()) = state_rx.changed() => {
                let state = state_rx.borrow();
                LoopRes::Msg(WebSocketMessage::StateUpdate { state: state.clone() })
            }
            Ok(()) = shutdown_rx.changed() => {
                LoopRes::Break
            }
            Ok(()) = updated_rx.changed() => {
                LoopRes::Break
            }
        };

        let mut state_rx_changed_msg = None;

        match res {
            LoopRes::Msg(msg) => {
                let msg = serde_json::to_string(&msg)
                    .map_err(|e| e.to_string())
                    .unwrap();
                let res = stream.send(rocket_ws::Message::Text(msg)).await;
                if let Err(e) = res {
                    error!("Error sending message: {:?}", e);
                }
            }
            LoopRes::JobStart(job) => {
                let mut manager = manager_handle.lock().await;
                let msg = match manager.request_job(job).await {
                    Ok(_) => WebSocketMessage::RunStarted,
                    Err(why) => WebSocketMessage::RunDenied { reason: why },
                };
                drop(manager);
                let msg = serde_json::to_string(&msg)
                    .map_err(|e| e.to_string())
                    .unwrap();
                let res = stream.send(rocket_ws::Message::Text(msg)).await;
                if let Err(e) = res {
                    error!("Error sending message: {:?}", e);
                }
            }
            LoopRes::Ping => {
                let res = stream
                    .send(rocket_ws::Message::Ping(vec![5, 4, 2, 6, 7, 3, 2, 5, 3]))
                    .await;
                if let Err(e) = res {
                    error!("Error sending ping: {:?}", e);
                }
            }
            LoopRes::Pong(e) => {
                let res = stream.send(rocket_ws::Message::Pong(e)).await;
                if let Err(e) = res {
                    error!("Error sending pong: {:?}", e);
                }
            }
            LoopRes::Break => {
                break;
            }
            LoopRes::ChangeJobRx(rx) => {
                state_rx = rx;
                let state = state_rx.borrow();
                let msg = serde_json::to_string(&WebSocketMessage::StateUpdate {
                    state: state.clone(),
                })
                .map_err(|e| e.to_string())
                .unwrap();
                state_rx_changed_msg = Some(msg);
            }
            _ => {}
        }

        if let Some(msg) = state_rx_changed_msg {
            let res = stream.send(rocket_ws::Message::Text(msg)).await;
            if let Err(e) = res {
                error!("Error sending message: {:?}", e);
            }
        }
    }
}

#[get("/ws/<contest_id>/<problem_id>")]
pub async fn ws_channel(
    ws: WebSocket,
    contest_id: i64,
    problem_id: i64,
    user: &User,
    admin: Option<&Admin>,
    manager: &State<ManagerHandle>,
    mut db: DbConnection,
) -> ResultResponse<rocket_ws::Channel<'static>> {
    Contest::get_or_404_assert_started(&mut db, contest_id, Some(user), admin).await?;
    let problem = Problem::by_id(&mut db, contest_id, problem_id)
        .await?
        .ok_or(Status::NotFound)?;

    let handle = (*manager).clone();
    let cases = TestCase::get_for_problem(&mut db, problem_id).await?;
    if !cases.is_empty() {
        let user_id = user.id;
        Ok(ws.channel(move |stream| {
            Box::pin(async move {
                websocket_loop(stream, handle, problem, cases, user_id).await;
                Ok(())
            })
        }))
    } else {
        Err(Status::NotFound.into())
    }
}
