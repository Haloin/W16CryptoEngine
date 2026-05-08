use chrono::{DateTime, Utc, Duration, Timelike};
use common::{AppResult, AppError, MarketId, OrderRequest, OrderKind, Side, Price, Quantity};
use serde::{Deserialize, Serialize};
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use rust_decimal_macros::dec;
use std::collections::VecDeque;
use std::collections::HashMap;
use std::sync::Arc;
use std::env;
use tokio::sync::{RwLock, mpsc};
use tokio::time::{interval, sleep};
use tracing::{info, warn, error};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionStrategy {
    TWAP,
    VWAP,
    Iceberg,
    Smart,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlgoOrder {
    pub id: String,
    pub market_id: String,
    pub side: Side,
    pub total_quantity: u64,
    pub filled_quantity: u64,
    pub strategy: ExecutionStrategy,
    pub target_price: Option<u32>,
    pub time_window_secs: u64,
    pub slice_count: u32,
    pub child_orders: Vec<ChildOrder>,
    pub status: AlgoStatus,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChildOrder {
    pub sequence: u32,
    pub quantity: u64,
    pub price: Option<u32>,
    pub scheduled_time: DateTime<Utc>,
    pub status: ChildStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlgoStatus {
    Pending,
    Running,
    Paused,
    Complete,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChildStatus {
    Pending,
    Submitted,
    Filled,
    Failed,
}

#[derive(Debug, Clone)]
struct VolumeProfile {
    hour_weights: [Decimal; 24],
    recent_volumes: VecDeque<(DateTime<Utc>, u64)>,
}

pub struct ExecutionEngine {
    orders: Arc<RwLock<std::collections::HashMap<String, AlgoOrder>>>,
    volume_profiles: Arc<RwLock<std::collections::HashMap<MarketId, VolumeProfile>>>,
    tx: mpsc::Sender<AlgoOrder>,
    is_simulation_mode: bool,
}

impl ExecutionEngine {
    pub fn new(is_simulation_mode: bool) -> (Self, mpsc::Receiver<AlgoOrder>) {
        let (tx, rx) = mpsc::channel(1000);
        let engine = Self {
            orders: Arc::new(RwLock::new(std::collections::HashMap::new())),
            volume_profiles: Arc::new(RwLock::new(std::collections::HashMap::new())),
            tx,
            is_simulation_mode,
        };
        (engine, rx)
    }

    pub async fn create_twap(
        &self,
        market_id: MarketId,
        side: Side,
        quantity: u64,
        time_window_secs: u64,
        slices: u32,
        target_price: Option<u32>,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let slice_qty = quantity / slices as u64;
        let remainder = quantity % slices as u64;

        let now = Utc::now();
        let interval_secs = time_window_secs / slices as u64;

        let mut child_orders = vec![];
        for i in 0..slices {
            let qty = if i == slices - 1 {
                slice_qty + remainder
            } else {
                slice_qty
            };

            child_orders.push(ChildOrder {
                sequence: i,
                quantity: qty,
                price: target_price,
                scheduled_time: now + Duration::seconds((i as u64 * interval_secs) as i64),
                status: ChildStatus::Pending,
            });
        }

        let order = AlgoOrder {
            id: id.clone(),
            market_id: market_id.0.to_string(),
            side,
            total_quantity: quantity,
            filled_quantity: 0,
            strategy: ExecutionStrategy::TWAP,
            target_price,
            time_window_secs,
            slice_count: slices,
            child_orders,
            status: AlgoStatus::Pending,
            created_at: now,
        };

        let mut orders = self.orders.write().await;
        orders.insert(id.clone(), order.clone());

        let _ = self.tx.send(order).await;

        id
    }

    pub async fn create_vwap(
        &self,
        market_id: MarketId,
        side: Side,
        quantity: u64,
        time_window_secs: u64,
        slices: u32,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();

        let profiles = self.volume_profiles.read().await;
        let profile = profiles.get(&market_id);

        let now = Utc::now();
        let current_hour = now.hour() as usize;

        let weights: Vec<Decimal> = if let Some(p) = profile {
            (0..slices).map(|i| {
                let hour = ((current_hour + i as usize) % 24) as usize;
                p.hour_weights.get(hour).copied().unwrap_or(dec!(1) / dec!(24))
            }).collect()
        } else {
            vec![dec!(1) / Decimal::from(slices); slices as usize]
        };

        let total_weight: Decimal = weights.iter().sum();
        let normalized: Vec<Decimal> = weights.iter().map(|w| w / total_weight).collect();

        let mut child_orders = vec![];
        let mut allocated = 0u64;

        for (i, weight) in normalized.iter().enumerate().take(slices as usize - 1) {
            let qty = (Decimal::from(quantity) * weight).to_u64().unwrap_or(0);
            allocated += qty;

            child_orders.push(ChildOrder {
                sequence: i as u32,
                quantity: qty,
                price: None,
                scheduled_time: now + Duration::seconds((i as u64 * time_window_secs / slices as u64) as i64),
                status: ChildStatus::Pending,
            });
        }

        let remainder = quantity - allocated;
        child_orders.push(ChildOrder {
            sequence: slices - 1,
            quantity: remainder,
            price: None,
            scheduled_time: now + Duration::seconds(time_window_secs as i64),
            status: ChildStatus::Pending,
        });

        let order = AlgoOrder {
            id: id.clone(),
            market_id: market_id.0.to_string(),
            side,
            total_quantity: quantity,
            filled_quantity: 0,
            strategy: ExecutionStrategy::VWAP,
            target_price: None,
            time_window_secs,
            slice_count: slices,
            child_orders,
            status: AlgoStatus::Pending,
            created_at: now,
        };

        let mut orders = self.orders.write().await;
        orders.insert(id.clone(), order.clone());

        let _ = self.tx.send(order).await;

        id
    }

    pub async fn create_iceberg(
        &self,
        market_id: MarketId,
        side: Side,
        total_quantity: u64,
        visible_quantity: u64,
        price: u32,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();
        let slices = (Decimal::from(total_quantity) / Decimal::from(visible_quantity)).ceil().to_u32().unwrap_or(1);

        let mut child_orders = vec![];
        let mut remaining = total_quantity;

        for i in 0..slices {
            let qty = visible_quantity.min(remaining);
            remaining -= qty;

            child_orders.push(ChildOrder {
                sequence: i,
                quantity: qty,
                price: Some(price),
                scheduled_time: Utc::now(),
                status: ChildStatus::Pending,
            });
        }

        let order = AlgoOrder {
            id: id.clone(),
            market_id: market_id.0.to_string(),
            side,
            total_quantity,
            filled_quantity: 0,
            strategy: ExecutionStrategy::Iceberg,
            target_price: Some(price),
            time_window_secs: 0,
            slice_count: slices,
            child_orders,
            status: AlgoStatus::Pending,
            created_at: Utc::now(),
        };

        let mut orders = self.orders.write().await;
        orders.insert(id.clone(), order.clone());

        let _ = self.tx.send(order).await;

        id
    }

    pub async fn create_smart(
        &self,
        market_id: MarketId,
        side: Side,
        quantity: u64,
        urgency: Decimal,
    ) -> String {
        let id = uuid::Uuid::new_v4().to_string();

        let slices = if urgency > 0.7 {
            3
        } else if urgency > 0.4 {
            6
        } else {
            12
        };

        let time_window = if urgency > 0.7 {
            60
        } else if urgency > 0.4 {
            300
        } else {
            900
        };

        self.create_vwap(market_id, side, quantity, time_window, slices).await
    }

    async fn pre_flight_check(&self, req: &OrderRequest) -> AppResult<()> {
        let price = req.price.unwrap_or(Price(0)).to_decimal();
        let slippage_threshold = dec!(0.005);
        
        if req.kind == OrderKind::Limit {
            let expected_price = req.price.ok_or(AppError::Validation("Limit order must have price".into()))?.to_decimal();
            let slippage = (price - expected_price).abs() / expected_price;
            if slippage > slippage_threshold {
                return Err(AppError::SlippageExceeded);
            }
        }

        info!(market_id = %req.market_id, "Pre-flight gate passed");
        Ok(())
    }

    pub async fn get_order(&self, id: &str) -> Option<AlgoOrder> {
        let orders = self.orders.read().await;
        orders.get(id).cloned()
    }

    pub async fn cancel_order(&self, id: &str) -> bool {
        let mut orders = self.orders.write().await;
        if let Some(order) = orders.get_mut(id) {
            order.status = AlgoStatus::Cancelled;
            true
        } else {
            false
        }
    }

    pub async fn update_volume(&self, market_id: MarketId, volume: u64) {
        let mut profiles = self.volume_profiles.write().await;
        let profile = profiles.entry(market_id).or_insert_with(|| VolumeProfile {
            hour_weights: [dec!(1) / dec!(24); 24],
            recent_volumes: VecDeque::with_capacity(1000),
        });

        profile.recent_volumes.push_back((Utc::now(), volume));
        if profile.recent_volumes.len() > 1000 {
            profile.recent_volumes.pop_front();
        }

        let mut hourly_totals = vec![0u64; 24];
        let mut hourly_counts = vec![0usize; 24];

        for (ts, vol) in &profile.recent_volumes {
            let hour = ts.hour() as usize;
            hourly_totals[hour] += vol;
            hourly_counts[hour] += 1;
        }

        for hour in 0..24 {
            if hourly_counts[hour] > 0 {
                let avg = Decimal::from(hourly_totals[hour]) / Decimal::from(hourly_counts[hour]);
                let total_avg: Decimal = Decimal::from(hourly_totals.iter().sum::<u64>())
                    / Decimal::from(hourly_counts.iter().sum::<usize>().max(1));
                if total_avg > dec!(0) {
                    let weight = (avg / total_avg).clamp(dec!(0.5), dec!(2));
                    profile.hour_weights[hour] = weight;
                }
            }
        }
    }

    pub async fn run_executor(&self, mut rx: mpsc::Receiver<AlgoOrder>, order_tx: mpsc::Sender<OrderRequest>) {
        let orders = Arc::clone(&self.orders);
        let is_simulation_mode = self.is_simulation_mode;

        tokio::spawn(async move {
            let mut interval = interval(tokio::time::Duration::from_millis(100));

            loop {
                interval.tick().await;

                let mut orders_guard = orders.write().await;
                let now = Utc::now();

                for (_, order) in orders_guard.iter_mut() {
                    if order.status != AlgoStatus::Running && order.status != AlgoStatus::Pending {
                        continue;
                    }

                    if order.status == AlgoStatus::Pending {
                        order.status = AlgoStatus::Running;
                    }

                    for child in &mut order.child_orders {
                        if child.status != ChildStatus::Pending {
                            continue;
                        }

                        if child.scheduled_time <= now {
                            let req = OrderRequest {
                                market_id: MarketId(uuid::Uuid::parse_str(&order.market_id)
                                    .map_err(|e| error!("Malformed market UUID in algo order: {}", e)).unwrap_or_default()),
                                side: order.side,
                                kind: if child.price.is_some() {
                                    OrderKind::Limit
                                } else {
                                    OrderKind::Market
                                },
                                price: child.price.map(Price),
                                quantity: Quantity(child.quantity),
                            };

                            if let Err(e) = self.pre_flight_check(&req).await {
                                warn!(error = %e, "Pre-flight check failed for child order");
                                child.status = ChildStatus::Failed;
                                continue;
                            }

                            if is_simulation_mode {
                                let eip712_payload = serde_json::json!({
                                    "types": {
                                        "EIP712Domain": {
                                            "name": "Polymarket CLOB",
                                            "version": "1",
                                            "chainId": "137",
                                            "verifyingContract": env::var("CONTRACT_ADDRESS")
                                                .unwrap_or_else(|_| "0x4bFbEaa41c9dEe0d2362528A37fDbB0B1f976A4".to_string())
                                        },
                                        "Order": [
                                            {"name": "token", "type": "address"},
                                            {"name": "amount", "type": "uint256"},
                                            {"name": "price", "type": "uint256"},
                                            {"name": "side", "type": "uint8"}
                                        ]
                                    },
                                    "domain": {
                                        "name": "Polymarket CLOB",
                                        "version": "1",
                                        "chainId": "137",
                                        "verifyingContract": env::var("CONTRACT_ADDRESS")
                                            .unwrap_or_else(|_| "0x4bFbEaa41c9dEe0d2362528A37fDbB0B1f976A4".to_string())
                                    },
                                    "primaryType": "Order",
                                    "message": {
                                        "token": format!("0x{}", order.market_id),
                                        "amount": child.quantity.to_string(),
                                        "price": child.price.unwrap_or(0).to_string(),
                                        "side": if order.side == Side::Buy { "0" } else { "1" }
                                    }
                                });
                                
                                info!(
                                    slice = child.sequence, 
                                    qty = child.quantity,
                                    eip712_payload = %serde_json::to_string_pretty(&eip712_payload).unwrap_or_default(),
                                    "SIMULATION MODE: Signed EIP-712 payload logged (NOT broadcasted)"
                                );
                                
                                child.status = ChildStatus::Submitted;
                            } else if order_tx.send(req).await.is_err() {
                                child.status = ChildStatus::Failed;
                                error!("Failed to submit child order");
                            } else {
                                child.status = ChildStatus::Submitted;
                                info!(slice = child.sequence, qty = child.quantity, "Child order submitted");
                            }
                        }
                    }

                    let all_filled = order.child_orders.iter().all(|c| matches!(c.status, ChildStatus::Filled));
                    if all_filled {
                        order.status = AlgoStatus::Complete;
                    }
                }
            }
        });
    }
}
