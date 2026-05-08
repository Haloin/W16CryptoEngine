use crate::AppState;
use analytics::{AnalyticsEngine, TradingSignalType};
use common::{AppError, MarketId, OrderId, OrderKind, Price, Quantity, Side, UserId, Fill, OrderBook, CircuitBreakerConfig};
use polymarket::{PolymarketClient, PolymarketWebSocket, MarketDataEvent};
use polymarket::websocket::MarketDataStream;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{RwLock, RwLockReadGuard};
use tracing::{error, info, warn};

pub struct TradingEngine {
    state: AppState,
    analytics: Arc<AnalyticsEngine>,
    polymarket_client: Arc<RwLock<Option<PolymarketClient>>>,
    ws_stream: Arc<RwLock<Option<MarketDataStream>>>,
    running: Arc<RwLock<bool>>,
}

impl TradingEngine {
    pub fn new(state: AppState, analytics: Arc<AnalyticsEngine>) -> Self {
        Self {
            state,
            analytics,
            polymarket_client: Arc::new(RwLock::new(None)),
            ws_stream: Arc::new(RwLock::new(None)),
            running: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn initialize(&self, api_key: String, api_secret: String, api_passphrase: String) -> Result<(), AppError> {
        let circuit_breaker_config = CircuitBreakerConfig {
            failure_threshold: 5,
            reset_timeout: Duration::from_secs(30),
            failure_window: Duration::from_secs(60),
        };

        let client = PolymarketClient::new(api_key, api_secret, api_passphrase)
            .map_err(|e| {
                error!("Failed to create Polymarket client: {}", e);
                e
            })?
            .with_circuit_breaker(circuit_breaker_config);
        
        let (ws, stream) = PolymarketWebSocket::new();
        
        *self.polymarket_client.write().await = Some(client);
        *self.ws_stream.write().await = Some(stream);
        
        info!("execution core initialized");
        Ok(())
    }

    pub async fn start(&self, market_ids: Vec<String>) -> Result<(), AppError> {
        *self.running.write().await = true;
        
        let ws_stream_guard: RwLockReadGuard<Option<MarketDataStream>> = self.ws_stream.read().await;
        if let Some(stream) = ws_stream_guard.as_ref() {
            for market_id in &market_ids {
                info!(market_id, "Subscribing to market data feeds");
            }
        }
        drop(ws_stream_guard);

        self.start_market_data_loop().await;
        self.start_signal_generation_loop().await;
        self.start_order_execution_loop().await;
        
        info!("execution engine active");
        Ok(())
    }


    async fn start_market_data_loop(&self) {
        let analytics = self.analytics.clone();
        let running = self.running.clone();
        let ws_stream = self.ws_stream.clone();

        tokio::spawn(async move {
            let mut stream = ws_stream.write().await;
            let Some(ref mut stream) = *stream else {
                error!("WebSocket stream not initialized");
                return;
            };

            while *running.read().await {
                if analytics.risk_manager.circuit_breaker.load(std::sync::atomic::Ordering::SeqCst) {
                    error!("RiskManager circuit breaker ACTIVE. Stopping market data loop.");
                    break;
                }

                let stream_ref: &mut MarketDataStream = stream;
                match stream_ref.recv().await {
                    Ok(event) => {
                        match event {
                            MarketDataEvent::OrderBook { market_id, asset_id: _, bids, asks, timestamp: _ } => {
                                let market_uuid = match uuid::Uuid::parse_str(&market_id) {
                                    Ok(id) => id,
                                    Err(e) => {
                                        warn!(market_id = %market_id, error = %e, "Invalid market_id format");
                                        continue;
                                    }
                                };
                                let book = OrderBook {
                                    market_id: MarketId(market_uuid),
                                    bids: bids.iter().map(|b| common::PriceLevel {
                                        price: common::Price((b.price * 1_000_000.0) as u32),
                                        quantity: common::Quantity((b.size * 1_000_000.0) as u64),
                                        count: 1,
                                    }).collect(),
                                    asks: asks.iter().map(|a| common::PriceLevel {
                                        price: common::Price((a.price * 1_000_000.0) as u32),
                                        quantity: common::Quantity((a.size * 1_000_000.0) as u64),
                                        count: 1,
                                    }).collect(),
                                    sequence: 0,
                                };
                                
                                analytics.on_book_update(book.market_id, &book).await;
                            }
                            MarketDataEvent::Trade { trade } => {
                                let trade_id = match uuid::Uuid::parse_str(&trade.id) {
                                    Ok(id) => id,
                                    Err(e) => {
                                        warn!(trade_id = %trade.id, error = %e, "Invalid trade_id format");
                                        continue;
                                    }
                                };
                                let market_id = match uuid::Uuid::parse_str(&trade.market_id) {
                                    Ok(id) => id,
                                    Err(e) => {
                                        warn!(market_id = %trade.market_id, error = %e, "Invalid market_id format");
                                        continue;
                                    }
                                };
                                let taker_order_id = match uuid::Uuid::parse_str(&trade.taker_order_id) {
                                    Ok(id) => id,
                                    Err(e) => {
                                        warn!(taker_order_id = %trade.taker_order_id, error = %e, "Invalid taker_order_id format");
                                        continue;
                                    }
                                };
                                let fill = Fill {
                                    id: common::FillId(trade_id),
                                    market_id: MarketId(market_id),
                                    maker_order_id: common::OrderId(uuid::Uuid::default()),
                                    taker_order_id: common::OrderId(taker_order_id),
                                    maker_user_id: common::UserId(uuid::Uuid::default()),
                                    taker_user_id: common::UserId(uuid::Uuid::default()),
                                    price: common::Price((trade.price * 1_000_000.0) as u32),
                                    quantity: common::Quantity(trade.size as u64),
                                    aggressor: match trade.side {
                                        polymarket::Side::Buy => common::Side::Buy,
                                        polymarket::Side::Sell => common::Side::Sell,
                                    },
                                    filled_at: chrono::Utc::now(),
                                    transaction_id: None,
                                    sequence: 0,
                                };
                                
                                analytics.on_fill(&fill).await;
                            }
                            _ => {}
                        }
                    }
                    Err(e) => {
                        warn!(error = %e, "WebSocket receive error");
                    }
                }
            }
        });
    }

    async fn start_signal_generation_loop(&self) {
        let analytics = self.analytics.clone();
        let running = self.running.clone();
        let polymarket_client = self.polymarket_client.clone();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_millis(100));
            
            while *running.read().await {
                if analytics.risk_manager.circuit_breaker.load(std::sync::atomic::Ordering::SeqCst) {
                    error!("RiskManager circuit breaker ACTIVE. Stopping signal generation.");
                    break;
                }
                interval.tick().await;
            }
        });
    }

    async fn start_order_execution_loop(&self) {
        let analytics = self.analytics.clone();
        let running = self.running.clone();
        let polymarket_client = self.polymarket_client.clone();
        let state = self.state.clone();

        tokio::spawn(async move {
            while *running.read().await {
                if analytics.risk_manager.circuit_breaker.load(std::sync::atomic::Ordering::SeqCst) {
                    error!("RiskManager circuit breaker ACTIVE. Canceling all orders and shutting down.");
                    
                    if let Err(e) = cancel_all_open_orders(&state).await {
                        error!(error = %e, "Failed to cancel orders during circuit breaker shutdown");
                    }
                    
                    break;
                }

                let market_id = common::MarketId(uuid::Uuid::new_v4());
                let signals = analytics.signal_gen.generate(market_id).await;

                for signal in signals {
                    if signal.strength > 0.8 && signal.confidence > 0.8 {
                        let client_guard = polymarket_client.read().await;
                        let Some(ref client) = *client_guard else {
                            continue;
                        };

                        let market = match client.get_market(&signal.market_id.0.to_string()).await {
                            Ok(m) => m,
                            Err(_) => continue,
                        };
                        
                        let Some(outcome) = market.outcomes.first() else {
                            continue;
                        };

                        continue;
                    }
                }
                
                tokio::time::sleep(tokio::time::Duration::from_millis(50)).await;
            }
        });
    }

    pub async fn get_status(&self) -> TradingEngineStatus {
        TradingEngineStatus {
            running: *self.running.read().await,
            connected: self.polymarket_client.read().await.is_some(),
            ws_connected: {
                let guard: tokio::sync::RwLockReadGuard<Option<MarketDataStream>> = self.ws_stream.read().await;
                guard.is_some()
            },
            circuit_breaker_active: self.analytics.risk_manager.circuit_breaker.load(std::sync::atomic::Ordering::SeqCst),
        }
    }

    pub async fn stop(&self) -> Result<(), anyhow::Error> {
        info!("shutting down trading engine...");
        *self.running.write().await = false;
        tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        
        info!("engine offline");
        Ok(())
    }

    pub fn spawn_graceful_shutdown_monitor(&self) -> tokio::task::JoinHandle<()> {
        let running = self.running.clone();
        let analytics = self.analytics.clone();
        tokio::spawn(async move {
            loop {
                if !*running.read().await {
                    break;
                }
                
                if analytics.risk_manager.circuit_breaker.load(std::sync::atomic::Ordering::SeqCst) {
                    error!("Circuit breaker detected in shutdown monitor");
                    break;
                }
                
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            }
        })
    }
}

async fn cancel_all_open_orders(state: &AppState) -> Result<(), AppError> {
    let active_orders = state.orders.list_all_active().await?;
    
    for order in active_orders {
        if let Err(e) = state.orders.cancel(order.user_id, order.id).await {
            error!(
                order_id = %order.id,
                user_id = %order.user_id,
                error = %e,
                "Failed to cancel order during circuit breaker shutdown"
            );
        }
    }
    
    info!("All open orders canceled due to circuit breaker activation");
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct TradingEngineStatus {
    pub running: bool,
    pub connected: bool,
    pub ws_connected: bool,
    pub circuit_breaker_active: bool,
}
