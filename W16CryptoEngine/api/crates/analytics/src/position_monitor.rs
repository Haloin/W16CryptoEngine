use chrono::{DateTime, Utc, Duration};
use common::{AppResult, FillId, MarketId, UserId, Position};
use polymarket::MarketDataEvent;
use db::Db;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

const MAX_LOSS_PERCENT: f64 = 0.05;
const MAX_POSITION_AGE_HOURS: i64 = 24;
const DRAWDOWN_WARNING_THRESHOLD: f64 = 0.03;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum AlertType {
    StopLoss,
    TakeProfit,
    MaxDrawdown,
    PositionExpired,
    UnusualSize,
    ConcentrationRisk,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionAlert {
    pub user_id: String,
    pub market_id: String,
    pub alert_type: AlertType,
    pub message: String,
    pub severity: AlertSeverity,
    pub position_size: i64,
    pub unrealized_pnl: Option<i64>,
    pub entry_price: Option<u32>,
    pub current_price: Option<u32>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

struct PositionData {
    net_quantity: i64,
    entry_prices: Vec<(u32, u64)>,
    last_fill_time: DateTime<Utc>,
    max_observed_value: i64,
    current_value: i64,
}

pub struct PositionMonitor {
    db: Arc<Db>,
    positions: Arc<RwLock<HashMap<(UserId, MarketId), PositionData>>>,
}

impl PositionMonitor {
    pub fn new(db: Arc<Db>) -> Self {
        Self {
            db,
            positions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn update_position(&self, user_id: UserId, market_id: MarketId, fill_id: FillId, price: u32, quantity: u64, side: i64) {
        let mut positions = self.positions.write().await;
        let key = (user_id, market_id);

        let pos = positions.entry(key).or_insert_with(|| PositionData {
            net_quantity: 0,
            entry_prices: vec![],
            last_fill_time: Utc::now(),
            max_observed_value: 0,
            current_value: 0,
        });

        let delta = if side > 0 { quantity as i64 } else { -(quantity as i64) };
        pos.net_quantity += delta;

        if delta > 0 {
            pos.entry_prices.push((price, quantity));
        } else {
            let to_remove = quantity.min(pos.entry_prices.iter().map(|(_, q)| *q).sum::<u64>());
            let mut remaining = to_remove;

            pos.entry_prices.retain(|(p, q)| {
                if remaining == 0 {
                    true
                } else {
                    let remove = (*q).min(remaining);
                    remaining -= remove;
                    remove < *q
                }
            });
        }

        pos.current_value = pos.net_quantity * price as i64;
        pos.max_observed_value = pos.max_observed_value.max(pos.current_value);
        pos.last_fill_time = Utc::now();

        if pos.net_quantity == 0 {
            positions.remove(&key);
        }
    }

    pub async fn check_all(&self) -> Vec<PositionAlert> {
        let positions = self.positions.read().await;
        let mut alerts = vec![];
        let now = Utc::now();

        for ((user_id, market_id), pos) in positions.iter() {
            let age_hours = (now - pos.last_fill_time).num_hours();

            if age_hours > MAX_POSITION_AGE_HOURS {
                alerts.push(PositionAlert {
                    user_id: user_id.0.to_string(),
                    market_id: market_id.0.to_string(),
                    alert_type: AlertType::PositionExpired,
                    message: format!("Position held for {} hours", age_hours),
                    severity: AlertSeverity::Warning,
                    position_size: pos.net_quantity,
                    unrealized_pnl: None,
                    entry_price: pos.entry_prices.first().map(|(p, _)| *p),
                    current_price: None,
                    timestamp: now,
                });
            }

            if pos.net_quantity.abs() > 1_000_000_000 {
                alerts.push(PositionAlert {
                    user_id: user_id.0.to_string(),
                    market_id: market_id.0.to_string(),
                    alert_type: AlertType::UnusualSize,
                    message: format!("Position size {} exceeds threshold", pos.net_quantity),
                    severity: AlertSeverity::Warning,
                    position_size: pos.net_quantity,
                    unrealized_pnl: None,
                    entry_price: None,
                    current_price: None,
                    timestamp: now,
                });
            }

            if pos.max_observed_value > 0 {
                let drawdown = (pos.max_observed_value - pos.current_value) as f64 / pos.max_observed_value as f64;

                if drawdown > MAX_LOSS_PERCENT {
                    alerts.push(PositionAlert {
                        user_id: user_id.0.to_string(),
                        market_id: market_id.0.to_string(),
                        alert_type: AlertType::MaxDrawdown,
                        message: format!("Drawdown {:.1}% exceeds max loss", drawdown * 100.0),
                        severity: AlertSeverity::Critical,
                        position_size: pos.net_quantity,
                        unrealized_pnl: Some(pos.current_value - pos.max_observed_value),
                        entry_price: None,
                        current_price: None,
                        timestamp: now,
                    });
                } else if drawdown > DRAWDOWN_WARNING_THRESHOLD {
                    alerts.push(PositionAlert {
                        user_id: user_id.0.to_string(),
                        market_id: market_id.0.to_string(),
                        alert_type: AlertType::MaxDrawdown,
                        message: format!("Drawdown {:.1}% warning", drawdown * 100.0),
                        severity: AlertSeverity::Warning,
                        position_size: pos.net_quantity,
                        unrealized_pnl: Some(pos.current_value - pos.max_observed_value),
                        entry_price: None,
                        current_price: None,
                        timestamp: now,
                    });
                }
            }
        }

        alerts
    }

    pub async fn check_concentration(&self, user_id: UserId) -> Vec<PositionAlert> {
        let positions = self.positions.read().await;
        let user_positions: Vec<_> = positions
            .iter()
            .filter(|((uid, _), _)| *uid == user_id)
            .map(|((_, mid), pos)| (*mid, pos.net_quantity.abs()))
            .collect();

        if user_positions.is_empty() {
            return vec![];
        }

        let total: i64 = user_positions.iter().map(|(_, q)| *q).sum();
        let mut alerts = vec![];

        for (market_id, size) in user_positions {
            let concentration = size as f64 / total as f64;

            if concentration > 0.5 {
                alerts.push(PositionAlert {
                    user_id: user_id.0.to_string(),
                    market_id: market_id.0.to_string(),
                    alert_type: AlertType::ConcentrationRisk,
                    message: format!("{:.1}% of portfolio concentrated here", concentration * 100.0),
                    severity: AlertSeverity::Warning,
                    position_size: size,
                    unrealized_pnl: None,
                    entry_price: None,
                    current_price: None,
                    timestamp: Utc::now(),
                });
            }
        }

        alerts
    }

    pub async fn calculate_unrealized_pnl(&self, user_id: UserId, market_id: MarketId, current_price: u32) -> Option<i64> {
        let positions = self.positions.read().await;
        let pos = positions.get(&(user_id, market_id))?;

        if pos.entry_prices.is_empty() {
            return None;
        }

        let total_qty: u64 = pos.entry_prices.iter().map(|(_, q)| q).sum();
        let avg_entry: u32 = if total_qty > 0 {
            (pos.entry_prices.iter().map(|(p, q)| *p as u64 * q).sum::<u64>() / total_qty) as u32
        } else {
            0
        };

        let pnl = (current_price as i64 - avg_entry as i64) * pos.net_quantity;
        Some(pnl)
    }
}
