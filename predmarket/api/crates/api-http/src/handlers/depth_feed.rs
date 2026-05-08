use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::Response,
};
use common::MarketId;
use std::time::Duration;
use tokio::time::interval;
use tracing::{info, warn};
use uuid::Uuid;

use crate::state::AppState;

pub async fn market_depth(
    ws:           WebSocketUpgrade,
    State(state): State<AppState>,
    Path(id):     Path<Uuid>,
) -> Response {
    ws.on_upgrade(move |socket| handle_depth(socket, state, MarketId(id)))
}

async fn handle_depth(mut socket: WebSocket, state: AppState, market_id: MarketId) {
    let sub = match state.nats.subscribe_depth(market_id).await {
        Ok(s) => s,
        Err(e) => {
            warn!(market_id = %market_id, error = %e, "ws: nats subscribe failed");
            return;
        }
    };

    info!(market_id = %market_id, "ws: depth client connected");

    let mut sub = sub;
    let mut ping_interval = interval(Duration::from_secs(30));

    loop {
        tokio::select! {
            _ = ping_interval.tick() => {
                let ping = serde_json::json!({"type": "ping"}).to_string();
                if socket.send(Message::Text(ping.into())).await.is_err() {
                    break;
                }
            }

            msg = sub.next() => {
                match msg {
                    Some(nats_msg) => {
                        let text = match std::str::from_utf8(&nats_msg.payload) {
                            Ok(s) => s.to_string(),
                            Err(_) => continue,
                        };

                        if socket.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    None => break,
                }
            }

            client_msg = socket.recv() => {
                match client_msg {
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(Message::Pong(_))) => {}
                    Some(Ok(Message::Ping(p))) => {
                        if socket.send(Message::Pong(p)).await.is_err() {
                            break;
                        }
                    }
                    Some(Err(e)) => {
                        warn!(error = %e, "ws: client error");
                        break;
                    }
                    _ => {}
                }
            }
        }
    }

    info!(market_id = %market_id, "ws: depth client disconnected");
}
