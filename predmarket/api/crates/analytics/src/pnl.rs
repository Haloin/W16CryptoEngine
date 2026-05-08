use common::{MarketId, UserId, Fill, Side};
use polymarket::MarketDataEvent;
use chrono::{DateTime, Utc, NaiveDate};
use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use rust_decimal_macros::dec;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

#[derive(Debug, Clone)]
pub struct PnLRecord {
    pub timestamp: DateTime<Utc>,
    pub market_id: MarketId,
    pub realized_pnl_usd: Decimal,
    pub unrealized_pnl_usd: Decimal,
    pub total_pnl_usd: Decimal,
    pub volume_traded: Decimal,
    pub num_trades: u64,
    pub avg_trade_size: Decimal,
    pub sharpe_ratio: Option<Decimal>,
}

#[derive(Debug, Clone)]
pub struct PositionPnL {
    pub market_id: MarketId,
    pub side: Side,
    pub entry_price: Decimal,
    pub current_price: Decimal,
    pub quantity: u64,
    pub realized_pnl: Decimal,
    pub unrealized_pnl: Decimal,
    pub opened_at: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct DailyPerformance {
    pub date: NaiveDate,
    pub starting_balance: Decimal,
    pub ending_balance: Decimal,
    pub total_pnl: Decimal,
    return_pct: Decimal,
    pub num_trades: u64,
    pub win_rate: Decimal,
    pub avg_win: Decimal,
    pub avg_loss: Decimal,
    pub max_drawdown_pct: Decimal,
    pub sharpe_ratio: Decimal,
}

pub struct PnLTracker {
    daily_records: Arc<RwLock<HashMap<NaiveDate, Vec<PnLRecord>>>>,
    positions: Arc<RwLock<HashMap<UserId, HashMap<MarketId, Vec<PositionPnL>>>>>,
    user_balances: Arc<RwLock<HashMap<UserId, Decimal>>>,
    historical_performance: Arc<RwLock<HashMap<UserId, Vec<DailyPerformance>>>>,
}

impl PnLTracker {
    pub fn new() -> Self {
        Self {
            daily_records: Arc::new(RwLock::new(HashMap::new())),
            positions: Arc::new(RwLock::new(HashMap::new())),
            user_balances: Arc::new(RwLock::new(HashMap::new())),
            historical_performance: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn record_fill(
        &self,
        user_id: UserId,
        market_id: MarketId,
        fill: &Fill,
        current_price: Decimal,
    ) {
        let mut positions = self.positions.write().await;
        let user_positions = positions.entry(user_id).or_insert_with(HashMap::new);
        let market_positions = user_positions.entry(market_id).or_insert_with(Vec::new);

        let fill_price = Decimal::from(fill.price.0) / dec!(1_000_000);
        let fill_qty = fill.quantity.0;

        if fill.aggressor == Side::Buy {
            market_positions.push(PositionPnL {
                market_id,
                side: Side::Buy,
                entry_price: fill_price,
                current_price,
                quantity: fill_qty,
                realized_pnl: dec!(0),
                unrealized_pnl: (current_price - fill_price) * Decimal::from(fill_qty),
                opened_at: Utc::now(),
                last_updated: Utc::now(),
            });
        } else {
            let mut remaining_qty = fill_qty;
            let mut total_realized_pnl = dec!(0);

            market_positions.retain_mut(|pos| {
                if remaining_qty == 0 || pos.side != Side::Buy {
                    return true;
                }

                let close_qty = std::cmp::min(remaining_qty, pos.quantity);
                let realized = (fill_price - pos.entry_price) * Decimal::from(close_qty);
                total_realized_pnl += realized;
                remaining_qty -= close_qty;

                if close_qty < pos.quantity {
                    pos.quantity -= close_qty;
                    true
                } else {
                    false
                }
            });

            self.update_balance(user_id, total_realized_pnl).await;
        }

        self.update_daily_record(user_id, market_id, current_price).await;
    }

    pub async fn update_prices(&self, market_id: MarketId, new_price: Decimal) {
        let mut positions = self.positions.write().await;
        
        for (_, user_positions) in positions.iter_mut() {
            if let Some(market_positions) = user_positions.get_mut(&market_id) {
                for pos in market_positions.iter_mut() {
                    pos.current_price = new_price;
                    pos.last_updated = Utc::now();
                    
                    pos.unrealized_pnl = match pos.side {
                        Side::Buy => (new_price - pos.entry_price) * Decimal::from(pos.quantity),
                        Side::Sell => (pos.entry_price - new_price) * Decimal::from(pos.quantity),
                    };
                }
            }
        }
    }

    pub async fn get_user_pnl(&self, user_id: UserId) -> (Decimal, Decimal) {
        let positions = self.positions.read().await;
        let user_positions = positions.get(&user_id);

        if let Some(user_positions) = user_positions {
            let mut total_realized = dec!(0);
            let mut total_unrealized = dec!(0);

            for (_, market_positions) in user_positions {
                for pos in market_positions {
                    total_realized += pos.realized_pnl;
                    total_unrealized += pos.unrealized_pnl;
                }
            }

            (total_realized, total_unrealized)
        } else {
            (dec!(0), dec!(0))
        }
    }

    pub async fn get_market_pnl(&self, user_id: UserId, market_id: MarketId) -> (Decimal, Decimal) {
        let positions = self.positions.read().await;
        
        positions
            .get(&user_id)
            .and_then(|user_positions| user_positions.get(&market_id))
            .map(|market_positions| {
                let realized: Decimal = market_positions.iter().map(|p| p.realized_pnl).sum();
                let unrealized: Decimal = market_positions.iter().map(|p| p.unrealized_pnl).sum();
                (realized, unrealized)
            })
            .unwrap_or((dec!(0), dec!(0)))
    }

    pub async fn get_daily_summary(&self, date: NaiveDate) -> Option<Vec<PnLRecord>> {
        let records = self.daily_records.read().await;
        records.get(&date).cloned()
    }

    pub async fn get_performance_metrics(&self, user_id: UserId) -> Option<PerformanceMetrics> {
        let history = self.historical_performance.read().await;
        let user_history = history.get(&user_id)?;

        if user_history.is_empty() {
            return None;
        }

        let total_return: Decimal = user_history.iter().map(|d| d.return_pct).sum();
        let avg_return = total_return / Decimal::from(user_history.len());
        
        let variance: Decimal = user_history
            .iter()
            .map(|d| (d.return_pct - avg_return) * (d.return_pct - avg_return))
            .sum::<Decimal>()
            / Decimal::from(user_history.len());
        
        let volatility = variance.sqrt();
        let sharpe_ratio = if volatility > dec!(0) {
            avg_return / volatility
        } else {
            dec!(0)
        };

        let total_trades: u64 = user_history.iter().map(|d| d.num_trades).sum();
        let avg_win_rate = user_history.iter().map(|d| d.win_rate).sum::<Decimal>()
            / Decimal::from(user_history.len());
        
        let max_drawdown = user_history
            .iter()
            .map(|d| d.max_drawdown_pct)
            .fold(dec!(0), |a, b| a.max(b));

        Some(PerformanceMetrics {
            total_return_pct: total_return,
            avg_daily_return_pct: avg_return,
            volatility_pct: volatility,
            sharpe_ratio,
            total_trades,
            win_rate_pct: avg_win_rate,
            max_drawdown_pct: max_drawdown,
            num_trading_days: user_history.len(),
        })
    }

    pub async fn end_trading_day(&self) {
        let mut daily_records = self.daily_records.write().await;
        let positions = self.positions.read().await;
        let balances = self.user_balances.read().await;
        
        let today = Utc::now().naive_utc().date();
        
        let mut daily_performances: HashMap<UserId, DailyPerformance> = HashMap::new();

        for (user_id, user_positions) in positions.iter() {
            let mut total_pnl = dec!(0);
            let mut num_trades = 0u64;
            let mut wins = 0u64;
            let mut losses = 0u64;
            let mut win_total = dec!(0);
            let mut loss_total = dec!(0);

            for (market_id, market_positions) in user_positions {
                for pos in market_positions {
                    total_pnl += pos.realized_pnl + pos.unrealized_pnl;
                    num_trades += 1;
                    
                    let trade_pnl = pos.realized_pnl + pos.unrealized_pnl;
                    if trade_pnl > dec!(0) {
                        wins += 1;
                        win_total += trade_pnl;
                    } else if trade_pnl < dec!(0) {
                        losses += 1;
                        loss_total += trade_pnl.abs();
                    }
                }
            }

            let win_rate = if num_trades > 0 {
                Decimal::from(wins) / Decimal::from(num_trades)
            } else {
                dec!(0)
            };

            let avg_win = if wins > 0 { win_total / Decimal::from(wins) } else { dec!(0) };
            let avg_loss = if losses > 0 { loss_total / Decimal::from(losses) } else { dec!(0) };

            let starting_balance = *balances.get(user_id).unwrap_or(&dec!(0));
            let ending_balance = starting_balance + total_pnl;
            
            let return_pct = if starting_balance > dec!(0) {
                total_pnl / starting_balance
            } else {
                dec!(0)
            };

            let performance = DailyPerformance {
                date: today,
                starting_balance,
                ending_balance,
                total_pnl,
                return_pct,
                num_trades,
                win_rate,
                avg_win,
                avg_loss,
                max_drawdown_pct: dec!(0),
                sharpe_ratio: dec!(0),
            };

            daily_performances.insert(*user_id, performance);
        }

        let mut historical = self.historical_performance.write().await;
        for (user_id, performance) in daily_performances {
            historical
                .entry(user_id)
                .or_insert_with(Vec::new)
                .push(performance);
        }

        info!("Trading day ended, performance recorded");
    }

    async fn update_balance(&self, user_id: UserId, pnl: Decimal) {
        let mut balances = self.user_balances.write().await;
        let balance = balances.entry(user_id).or_insert(dec!(0));
        *balance += pnl;
    }

    async fn update_daily_record(&self, user_id: UserId, market_id: MarketId, current_price: Decimal) {
        let today = Utc::now().naive_utc().date();
        let (realized, unrealized) = self.get_market_pnl(user_id, market_id).await;
        
        let record = PnLRecord {
            timestamp: Utc::now(),
            market_id,
            realized_pnl_usd: realized,
            unrealized_pnl_usd: unrealized,
            total_pnl_usd: realized + unrealized,
            volume_traded: dec!(0),
            num_trades: 1,
            avg_trade_size: dec!(0),
            sharpe_ratio: None,
        };

        let mut records = self.daily_records.write().await;
        records
            .entry(today)
            .or_insert_with(Vec::new)
            .push(record);
    }
}

#[derive(Debug, Clone)]
pub struct PerformanceMetrics {
    pub total_return_pct: Decimal,
    pub avg_daily_return_pct: Decimal,
    pub volatility_pct: Decimal,
    pub sharpe_ratio: Decimal,
    pub total_trades: u64,
    pub win_rate_pct: Decimal,
    pub max_drawdown_pct: Decimal,
    pub num_trading_days: usize,
}
