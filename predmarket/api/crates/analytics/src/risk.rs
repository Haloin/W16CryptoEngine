use common::{MarketId, UserId, Order, Fill, Side, Position, AppError};
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use rust_decimal_macros::dec;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

const MAX_POSITION_SIZE_USD: Decimal = dec!(100000.0);
const MAX_DAILY_LOSS_USD: Decimal = dec!(50000.0);
const MAX_TOTAL_EXPOSURE_USD: Decimal = dec!(500000.0);
const MAX_DRAWDOWN_PCT: Decimal = dec!(0.20);
const MAX_POSITIONS_PER_MARKET: usize = 5;
const MAX_CORRELATED_EXPOSURE: Decimal = dec!(200000.0);

#[derive(Debug, Clone)]
pub struct RiskLimits {
    pub max_position_size_usd: Decimal,
    pub max_daily_loss_usd: Decimal,
    pub max_total_exposure_usd: Decimal,
    pub max_drawdown_pct: Decimal,
    pub max_positions_per_market: usize,
    pub max_correlated_exposure: Decimal,
}

impl Default for RiskLimits {
    fn default() -> Self {
        Self {
            max_position_size_usd: MAX_POSITION_SIZE_USD,
            max_daily_loss_usd: MAX_DAILY_LOSS_USD,
            max_total_exposure_usd: MAX_TOTAL_EXPOSURE_USD,
            max_drawdown_pct: MAX_DRAWDOWN_PCT,
            max_positions_per_market: MAX_POSITIONS_PER_MARKET,
            max_correlated_exposure: MAX_CORRELATED_EXPOSURE,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RiskCheckResult {
    pub allowed: bool,
    pub reason: Option<String>,
    pub risk_score: Decimal,
}

#[derive(Debug, Clone)]
pub struct UserRiskState {
    pub user_id: UserId,
    pub daily_pnl_usd: Decimal,
    pub total_exposure_usd: Decimal,
    pub max_drawdown_pct: Decimal,
    pub positions: HashMap<MarketId, Vec<Position>>,
    pub fills_today: Vec<Fill>,
    pub peak_balance_usd: Decimal,
    pub current_balance_usd: Decimal,
}

pub struct RiskManager {
    limits: RiskLimits,
    user_states: Arc<RwLock<HashMap<UserId, UserRiskState>>>,
    pub circuit_breaker: Arc<AtomicBool>,
}

impl RiskManager {
    pub fn new(limits: RiskLimits) -> Self {
        Self {
            limits,
            user_states: Arc::new(RwLock::new(HashMap::new())),
            circuit_breaker: Arc::new(AtomicBool::new(false)),
        }
    }

    pub async fn check_order_risk(
        &self,
        user_id: UserId,
        order: &Order,
        current_price: rust_decimal::Decimal,
    ) -> RiskCheckResult {
        let states = self.user_states.read().await;
        let user_state = states.get(&user_id);

        if self.circuit_breaker.load(std::sync::atomic::Ordering::Relaxed) {
            return RiskCheckResult {
                allowed: false,
                reason: Some("Global circuit breaker is active".to_string()),
                risk_score: dec!(1.0),
            };
        }

        let order_value = Decimal::from(order.quantity.0) * current_price;

        if order_value > self.limits.max_position_size_usd {
            return RiskCheckResult {
                allowed: false,
                reason: Some(format!(
                    "Order value ${} exceeds max position size ${}",
                    order_value, self.limits.max_position_size_usd
                )),
                risk_score: dec!(1.0),
            };
        }

        if let Some(state) = user_state {
            let new_exposure = state.total_exposure_usd + order_value;
            if new_exposure > self.limits.max_total_exposure_usd {
                return RiskCheckResult {
                    allowed: false,
                    reason: Some(format!(
                        "Total exposure ${} would exceed limit ${}",
                        new_exposure, self.limits.max_total_exposure_usd
                    )),
                    risk_score: dec!(1.0),
                };
            }

            let market_positions = state.positions.get(&order.market_id).map(|p: &Vec<_>| p.len()).unwrap_or(0);
            if market_positions >= self.limits.max_positions_per_market {
                return RiskCheckResult {
                    allowed: false,
                    reason: Some(format!(
                        "Max positions ({}) reached for this market",
                        self.limits.max_positions_per_market
                    )),
                    risk_score: dec!(1.0),
                };
            }

            if state.daily_pnl_usd < -self.limits.max_daily_loss_usd {
                return RiskCheckResult {
                    allowed: false,
                    reason: Some(format!(
                        "Daily loss limit exceeded: ${}",
                        state.daily_pnl_usd
                    )),
                    risk_score: dec!(1.0),
                };
            }

            let drawdown = if state.peak_balance_usd > dec!(0.0) {
                (state.peak_balance_usd - state.current_balance_usd) / state.peak_balance_usd
            } else {
                dec!(0.0)
            };

            if drawdown > self.limits.max_drawdown_pct {
                return RiskCheckResult {
                    allowed: false,
                    reason: Some(format!(
                        "Max drawdown exceeded: {}%",
                        drawdown * dec!(100.0)
                    )),
                    risk_score: dec!(1.0),
                };
            }

            let risk_score = self.calculate_risk_score(state, order, order_value);
            if risk_score > dec!(0.8) {
                return RiskCheckResult {
                    allowed: false,
                    reason: Some("Risk score too high".to_string()),
                    risk_score,
                };
            }

            RiskCheckResult {
                allowed: true,
                reason: None,
                risk_score,
            }
        } else {
            RiskCheckResult {
                allowed: true,
                reason: None,
                risk_score: dec!(0.0),
            }
        }
    }

    pub async fn update_on_fill(&self, user_id: UserId, fill: &Fill, current_price: rust_decimal::Decimal) {
        let mut states = self.user_states.write().await;
        let state = states.entry(user_id).or_insert_with(|| UserRiskState {
            user_id,
            daily_pnl_usd: dec!(0.0),
            total_exposure_usd: dec!(0.0),
            max_drawdown_pct: dec!(0.0),
            positions: HashMap::new(),
            fills_today: Vec::new(),
            peak_balance_usd: dec!(0.0),
            current_balance_usd: dec!(0.0),
        });

        let fill_value = Decimal::from(fill.quantity.0) * current_price;
        
        if fill.aggressor == Side::Buy {
            state.total_exposure_usd += fill_value;
        } else {
            state.total_exposure_usd -= fill_value;
        }

        state.fills_today.push(fill.clone());

        self.update_pnl(state, current_price).await;
        
        info!(
            user_id = %user_id,
            fill_id = %fill.id,
            daily_pnl = state.daily_pnl_usd,
            exposure = state.total_exposure_usd,
            "Risk state updated on fill"
        );
    }

    pub async fn get_user_exposure(&self, user_id: UserId) -> Decimal {
        let states = self.user_states.read().await;
        states
            .get(&user_id)
            .map(|s| s.total_exposure_usd)
            .unwrap_or(dec!(0.0))
    }

    pub async fn get_user_pnl(&self, user_id: UserId) -> Decimal {
        let states = self.user_states.read().await;
        states
            .get(&user_id)
            .map(|s| s.daily_pnl_usd)
            .unwrap_or(dec!(0.0))
    }

    pub async fn reset_daily_stats(&self) {
        let mut states = self.user_states.write().await;
        for (_, state) in states.iter_mut() {
            state.daily_pnl_usd = dec!(0.0);
            state.fills_today.clear();
            state.peak_balance_usd = state.current_balance_usd;
        }
        self.circuit_breaker.store(false, std::sync::atomic::Ordering::SeqCst);
        info!("Daily risk stats reset");
    }

    fn calculate_risk_score(
        &self,
        state: &UserRiskState,
        order: &Order,
        order_value: Decimal,
    ) -> Decimal {
        let exposure_ratio = (state.total_exposure_usd + order_value)
            / self.limits.max_total_exposure_usd;
        
        let drawdown_ratio = state.max_drawdown_pct / self.limits.max_drawdown_pct;
        
        let concentration_ratio = order_value
            / (state.total_exposure_usd + order_value + dec!(1.0));

        let pnl_ratio = if state.daily_pnl_usd < dec!(0.0) {
            (-state.daily_pnl_usd) / self.limits.max_daily_loss_usd
        } else {
            dec!(0.0)
        };

        let risk_score = (exposure_ratio * dec!(0.3))
            + (drawdown_ratio * dec!(0.3))
            + (concentration_ratio * dec!(0.2))
            + (pnl_ratio * dec!(0.2));

        risk_score.min(dec!(1.0))
    }

    async fn update_pnl(&self, state: &mut UserRiskState, current_price: Decimal) {
        let mut realized_pnl = dec!(0);
        let mut position_map: HashMap<(MarketId, Side), Vec<(u64, Decimal)>> = HashMap::new();
        let cur_price = current_price;

        for fill in &state.fills_today {
            let key = (fill.market_id, fill.aggressor.clone());
            position_map
                .entry(key)
                .or_insert_with(Vec::new)
                .push((fill.quantity.0, fill.price.0));
        }

        for ((market_id, side), fills) in position_map {
            let avg_entry = fills.iter().map(|(_, p)| p).sum::<Decimal>() / Decimal::from(fills.len().max(1));
            let total_qty: u64 = fills.iter().map(|(q, _)| q).sum();
            
            let market_pnl = if side == Side::Buy {
                (cur_price - avg_entry) * Decimal::from(total_qty)
            } else {
                (avg_entry - cur_price) * Decimal::from(total_qty)
            };
            
            realized_pnl += market_pnl;
        }

        state.daily_pnl_usd = realized_pnl;
        state.current_balance_usd += realized_pnl;
        
        if state.current_balance_usd > state.peak_balance_usd {
            state.peak_balance_usd = state.current_balance_usd;
        }
        
        if state.peak_balance_usd > dec!(0.0) {
            state.max_drawdown_pct = 
                (state.peak_balance_usd - state.current_balance_usd) / state.peak_balance_usd;
        }

        if state.daily_pnl_usd <= -self.limits.max_daily_loss_usd {
            self.circuit_breaker.store(true, std::sync::atomic::Ordering::SeqCst);
            error!(user_id = %state.user_id, pnl = %state.daily_pnl_usd, "MAX_DAILY_LOSS triggered circuit breaker");
        }
    }
}
