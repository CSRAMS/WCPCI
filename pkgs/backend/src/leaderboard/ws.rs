use log::error;
use rocket::{
    futures::{SinkExt, StreamExt},
    get, State,
};
use rocket_ws::{stream::DuplexStream, WebSocket};
use tokio::{
    select,
    time::{self, Duration, Instant},
};

use crate::{contests::Contest, db::DbConnection, error::prelude::*};

use super::{
    manager::{LeaderboardUpdateMessage, LeaderboardUpdateReceiver, ShutdownReceiver},
    LeaderboardManagerHandle,
};

enum LoopRes {
    NoOp,
    Break,
    Ping,
    Pong(Vec<u8>),
    Msg(LeaderboardUpdateMessage),
}

async fn websocket_loop(
    mut stream: DuplexStream,
    mut rx: LeaderboardUpdateReceiver,
    mut shutdown_rx: ShutdownReceiver,
) {
    let sleep = time::sleep(Duration::from_secs(10));
    tokio::pin!(sleep);

    loop {
        let res = select! {
            () = &mut sleep => {
                sleep.as_mut().reset(Instant::now() + Duration::from_secs(10));
                LoopRes::Ping
            },
            client_message = stream.next() => {
                if let Some(client_message) = client_message {
                    match client_message {
                        Ok(rocket_ws::Message::Close(_)) => LoopRes::Break,
                        Ok(rocket_ws::Message::Ping(data)) => LoopRes::Pong(data),
                        _ => LoopRes::NoOp
                    }
                } else {
                    LoopRes::Break
                }
            }
            leaderboard_update = rx.recv() => {
                match leaderboard_update {
                    Ok(msg) => LoopRes::Msg(msg),
                    Err(e) => {
                        error!("Error receiving leaderboard update: {:?}", e);
                        LoopRes::NoOp
                    }
                }
            }
            Ok(()) = shutdown_rx.changed() => {
                LoopRes::Break
            }
        };

        match res {
            LoopRes::Break => break,
            LoopRes::Msg(msg) => {
                let json_string = serde_json::to_string(&msg).unwrap();
                let res = stream.send(rocket_ws::Message::Text(json_string)).await;
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
            LoopRes::Pong(data) => {
                let res = stream.send(rocket_ws::Message::Pong(data)).await;
                if let Err(e) = res {
                    error!("Error sending pong: {:?}", e);
                }
            }
            _ => {}
        }
    }
}

#[get("/contests/<contest_id>/leaderboard/ws")]
pub async fn leaderboard_ws(
    ws: WebSocket,
    mut db: DbConnection,
    contest_id: i64,
    manager: &State<LeaderboardManagerHandle>,
) -> ResultResponse<rocket_ws::Channel<'static>> {
    let contest = Contest::get_or_404(&mut db, contest_id).await?;
    let mut manager = manager.lock().await;
    let rx = manager.subscribe_leaderboard(&mut db, &contest).await?;
    let shutdown_rx = manager.subscribe_shutdown();
    Ok(ws.channel(move |stream| {
        Box::pin(async move {
            websocket_loop(stream, rx, shutdown_rx).await;
            Ok(())
        })
    }))
}
