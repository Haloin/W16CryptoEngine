use common::AppError;
use crate::types::MarketDataEvent;
use futures::{stream::StreamExt, SinkExt};
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::{interval, Duration};
use tokio_tungstenite::{connect_async, WebSocketStream};
use tokio_tungstenite::tungstenite::Message;
use tracing::{debug, error, info};

const POLYMARKET_WS_URL: &str = "wss://clob.polymarket.com/ws";

pub struct PolymarketWebSocket {
    tx: mpsc::UnboundedSender<WsCommand>,
    market_data_tx: broadcast::Sender<MarketDataEvent>,
    connected: Arc<RwLock<bool>>,
}

#[derive(Debug)]
enum WsCommand {
    Subscribe { channel: String, market_id: String },
    Unsubscribe { channel: String, market_id: String },
    Ping,
    Close,
}

impl PolymarketWebSocket {
    pub fn new() -> (Self, MarketDataStream) {
        let (tx, rx) = mpsc::unbounded_channel();
        let (market_data_tx, market_data_rx) = broadcast::channel(1000);
        let connected = RwLock::new(false);

        let market_data_tx_clone = market_data_tx.clone();
        let ws = Self {
            tx,
            market_data_tx,
            connected: Arc::new(connected),
        };
        tokio::spawn(Self::connection_loop(rx, market_data_tx_clone, Arc::clone(&ws.connected)));

        let stream = MarketDataStream { rx: market_data_rx };
        (ws, stream)
    }

    pub async fn subscribe_orderbook(&self, market_id: &str, asset_id: &str) -> Result<(), AppError> {
        let channel = format!("orderbook:{}", asset_id);
        self.tx
            .send(WsCommand::Subscribe {
                channel,
                market_id: market_id.to_string(),
            })
            .map_err(|_| AppError::External("WebSocket closed".to_string()))
    }

    pub async fn subscribe_trades(&self, market_id: &str, asset_id: &str) -> Result<(), AppError> {
        let channel = format!("trades:{}", asset_id);
        self.tx
            .send(WsCommand::Subscribe {
                channel,
                market_id: market_id.to_string(),
            })
            .map_err(|_| AppError::External("WebSocket closed".to_string()))
    }

    pub async fn subscribe_prices(&self) -> Result<(), AppError> {
        self.tx
            .send(WsCommand::Subscribe {
                channel: "prices".to_string(),
                market_id: "*".to_string(),
            })
            .map_err(|_| AppError::External("WebSocket closed".to_string()))
    }

    pub async fn unsubscribe(&self, channel: &str, market_id: &str) -> Result<(), AppError> {
        self.tx
            .send(WsCommand::Unsubscribe {
                channel: channel.to_string(),
                market_id: market_id.to_string(),
            })
            .map_err(|_| AppError::External("WebSocket closed".to_string()))
    }

    pub async fn close(&self) -> Result<(), AppError> {
        self.tx
            .send(WsCommand::Close)
            .map_err(|_| AppError::Internal("WebSocket already closed".to_string()))
    }

    pub async fn is_connected(&self) -> bool {
        *self.connected.read().await
    }

    async fn connection_loop(
        mut rx: mpsc::UnboundedReceiver<WsCommand>,
        market_data_tx: broadcast::Sender<MarketDataEvent>,
        connected: Arc<RwLock<bool>>,
    ) {
        let mut retry_delay = Duration::from_secs(1);
        let max_retry_delay = Duration::from_secs(60);

        loop {
            match Self::connect_and_run(&mut rx, &market_data_tx, &connected).await {
                Ok(()) => {
                    info!("WebSocket connection closed gracefully");
                    break;
                }
                Err(e) => {
                    error!(error = %e, "WebSocket error, reconnecting...");
                    tokio::time::sleep(retry_delay).await;
                    retry_delay = std::cmp::min(retry_delay * 2, max_retry_delay);
                }
            }
        }
    }

    async fn connect_and_run(
        rx: &mut mpsc::UnboundedReceiver<WsCommand>,
        market_data_tx: &broadcast::Sender<MarketDataEvent>,
        connected: &Arc<RwLock<bool>>,
    ) -> Result<(), AppError> {
        let url = url::Url::parse(POLYMARKET_WS_URL).map_err(|e| {
            AppError::Internal(format!("Invalid WebSocket URL: {}", e))
        })?;

        let (ws_stream, _) = connect_async(url).await.map_err(|e| {
            AppError::Internal(format!("WebSocket connect failed: {}", e))
        })?;

        info!("WebSocket connected");
        *connected.write().await = true;

        let (mut write, mut read) = ws_stream.split();
        let mut ping_interval = interval(Duration::from_secs(30));

        loop {
            tokio::select! {
                Some(cmd) = rx.recv() => {
                    match cmd {
                        WsCommand::Subscribe { channel, market_id } => {
                            let msg = serde_json::json!({
                                "action": "subscribe",
                                "channel": channel,
                                "marketId": market_id,
                            });
                            if let Err(e) = write.send(Message::Text(msg.to_string())).await {
                                error!(error = %e, "Failed to send subscription request");
                                return Err(AppError::Internal("Failed to send subscription request".to_string()));
                            }
                            debug!(channel, market_id, "Subscribed to channel");
                        }
                        WsCommand::Unsubscribe { channel, market_id } => {
                            let msg = serde_json::json!({
                                "action": "unsubscribe",
                                "channel": channel,
                                "marketId": market_id,
                            });
                            if let Err(e) = write.send(Message::Text(msg.to_string())).await {
                                error!(error = %e, "Failed to send unsubscribe");
                            }
                        }
                        WsCommand::Ping => {
                            if let Err(e) = write.send(Message::Ping(vec![])).await {
                                error!(error = %e, "Failed to send ping");
                                return Err(AppError::Internal(format!("Ping error: {}", e)));
                            }
                        }
                        WsCommand::Close => {
                            let _ = write.close().await;
                            return Ok(());
                        }
                    }
                }

                _ = ping_interval.tick() => {
                    if let Err(e) = write.send(Message::Ping(vec![])).await {
                        error!(error = %e, "Ping failed");
                        return Err(AppError::Internal(format!("Ping failed: {}", e)));
                    }
                    debug!("Sent ping");
                }

                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if let Err(e) = Self::handle_message(&text, market_data_tx) {
                                debug!(error = %e, "Failed to handle message");
                            }
                        }
                        Some(Ok(Message::Pong(_))) => {
                            debug!("Received pong");
                        }
                        Some(Ok(Message::Close(_))) => {
                            info!("Received close frame");
                            return Ok(());
                        }
                        Some(Err(e)) => {
                            error!(error = %e, "WebSocket error");
                            return Err(AppError::Internal(format!("WebSocket error: {}", e)));
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    fn handle_message(
        text: &str,
        market_data_tx: &broadcast::Sender<MarketDataEvent>,
    ) -> Result<(), AppError> {
        let value: serde_json::Value = serde_json::from_str(text).map_err(|e| {
            AppError::Internal(format!("JSON parse failed: {}", e))
        })?;

        if let Some(event_type) = value.get("type").and_then(|t: &serde_json::Value| t.as_str()) {
            match event_type {
                "orderbook" => {
                    if let Ok(event) = serde_json::from_value::<MarketDataEvent>(value) {
                        let _ = market_data_tx.send(event);
                    }
                }
                "trade" => {
                    if let Ok(event) = serde_json::from_value::<MarketDataEvent>(value) {
                        let _ = market_data_tx.send(event);
                    }
                }
                "price_change" => {
                    if let Ok(event) = serde_json::from_value::<MarketDataEvent>(value) {
                        let _ = market_data_tx.send(event);
                    }
                }
                _ => {
                    debug!(event_type, "Unknown event type");
                }
            }
        }

        Ok(())
    }
}

pub struct MarketDataStream {
    rx: broadcast::Receiver<MarketDataEvent>,
}

impl MarketDataStream {
    pub async fn recv(&mut self) -> Result<MarketDataEvent, broadcast::error::RecvError> {
        self.rx.recv().await
    }

    pub fn try_recv(&mut self) -> Result<MarketDataEvent, broadcast::error::TryRecvError> {
        self.rx.try_recv()
    }
}
