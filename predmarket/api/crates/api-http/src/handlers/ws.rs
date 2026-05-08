use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Path, State,
    },
    response::Response,
};
use common::MarketId;
use futures_util::StreamExt;
use std::time::Duration;
use tokio::time::timeout;
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
        Ok(s)  => s,
        Err(e) => {
            warn!(market_id = %market_id, error = %e, "ws: nats subscribe failed");
            return;
        }
    };

    info!(market_id = %market_id, "ws: depth client connected");

    let ping_payload = serde_json::json!({"type": "ping"}).to_string();
    let mut sub = sub;

    loop {
        tokio::select! {
            msg = timeout(Duration::from_secs(30), async {
                None::<String>
            }) => {
                match msg {
                    Ok(Some(nats_msg)) => {
                        let text = nats_msg;
                        if socket.send(Message::Text(text.into())).await.is_err() {
                            break;
                        }
                    }
                    Ok(None) => break,
                    Err(_) => {
                        if socket.send(Message::Text(ping_payload.clone().into())).await.is_err() {
                            break;
                        }
                    }
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
