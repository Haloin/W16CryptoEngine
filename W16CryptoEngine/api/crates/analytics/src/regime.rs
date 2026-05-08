use chrono::{DateTime, Utc, Duration};
use common::{Fill, MarketId, OrderBook, Price};
use polymarket::MarketDataEvent;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

const VOLATILITY_WINDOW: usize = 50;
const TREND_WINDOW: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarketRegime {
    TrendingUp,
    TrendingDown,
    Ranging,
    HighVolatility,
    LowLiquidity,
    Normal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VolatilityRegime {
    Low,
    Medium,
    High,
    Extreme,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegimeState {
    pub market_id: String,
    pub regime: MarketRegime,
    pub volatility: VolatilityRegime,
    pub trend_strength: f64,
    pub volatility_measure: f64,
    pub liquidity_score: f64,
    pub timestamp: DateTime<Utc>,
}

struct MarketStats {
    prices: VecDeque<u32>,
    volumes: VecDeque<u64>,
    spreads: VecDeque<u32>,
    last_update: DateTime<Utc>,
}

pub struct MarketRegimeDetector {
    data: Arc<RwLock<HashMap<MarketId, MarketStats>>>,
}

impl MarketRegimeDetector {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn on_fill(&self, fill: &Fill) {
        let mut data = self.data.write().await;
        let stats = data.entry(fill.market_id).or_insert_with(|| MarketStats {
            prices: VecDeque::with_capacity(TREND_WINDOW),
            volumes: VecDeque::with_capacity(TREND_WINDOW),
            spreads: VecDeque::with_capacity(VOLATILITY_WINDOW),
            last_update: Utc::now(),
        });

        stats.prices.push_back(fill.price.0);
        stats.volumes.push_back(fill.quantity.0);

        if stats.prices.len() > TREND_WINDOW {
            stats.prices.pop_front();
        }
        if stats.volumes.len() > TREND_WINDOW {
            stats.volumes.pop_front();
        }

        stats.last_update = Utc::now();
    }

    pub async fn on_book_update(&self, market_id: MarketId, book: &OrderBook) {
        let mut data = self.data.write().await;
        let stats = data.entry(market_id).or_insert_with(|| MarketStats {
            prices: VecDeque::with_capacity(TREND_WINDOW),
            volumes: VecDeque::with_capacity(TREND_WINDOW),
            spreads: VecDeque::with_capacity(VOLATILITY_WINDOW),
            last_update: Utc::now(),
        });

        if let (Some(best_bid), Some(best_ask)) = (
            book.bids.first().map(|p| p.price),
            book.asks.first().map(|p| p.price),
        ) {
            let spread = best_ask.0.saturating_sub(best_bid.0);
            stats.spreads.push_back(spread);

            if stats.spreads.len() > VOLATILITY_WINDOW {
                stats.spreads.pop_front();
            }
        }

        stats.last_update = Utc::now();
    }

    pub async fn current_regime(&self, market_id: MarketId) -> MarketRegime {
        let data = self.data.read().await;
        let stats = match data.get(&market_id) {
            Some(s) => s,
            None => return MarketRegime::Normal,
        };

        if stats.prices.len() < 20 {
            return MarketRegime::Normal;
        }

        let volatility = self.calculate_volatility(&stats.prices);
        let trend = self.calculate_trend(&stats.prices);
        let liquidity = self.calculate_liquidity(&stats.spreads, &stats.volumes);

        if volatility > 0.05 {
            return MarketRegime::HighVolatility;
        }

        if liquidity < 0.3 {
            return MarketRegime::LowLiquidity;
        }

        if trend.abs() > 0.02 {
            if trend > 0.0 {
                MarketRegime::TrendingUp
            } else {
                MarketRegime::TrendingDown
            }
        } else {
            MarketRegime::Ranging
        }
    }

    pub async fn get_regime_state(&self, market_id: MarketId) -> Option<RegimeState> {
        let data = self.data.read().await;
        let stats = data.get(&market_id)?;

        if stats.prices.len() < 20 {
            return None;
        }

        let volatility = self.calculate_volatility(&stats.prices);
        let trend = self.calculate_trend(&stats.prices);
        let liquidity = self.calculate_liquidity(&stats.spreads, &stats.volumes);

        let vol_regime = if volatility > 0.08 {
            VolatilityRegime::Extreme
        } else if volatility > 0.04 {
            VolatilityRegime::High
        } else if volatility > 0.02 {
            VolatilityRegime::Medium
        } else {
            VolatilityRegime::Low
        };

        let regime = if volatility > 0.05 {
            MarketRegime::HighVolatility
        } else if liquidity < 0.3 {
            MarketRegime::LowLiquidity
        } else if trend.abs() > 0.02 {
            if trend > 0.0 {
                MarketRegime::TrendingUp
            } else {
                MarketRegime::TrendingDown
            }
        } else {
            MarketRegime::Ranging
        };

        Some(RegimeState {
            market_id: market_id.0.to_string(),
            regime,
            volatility: vol_regime,
            trend_strength: trend.abs(),
            volatility_measure: volatility,
            liquidity_score: liquidity,
            timestamp: Utc::now(),
        })
    }

    fn calculate_volatility(&self, prices: &VecDeque<u32>) -> f64 {
        if prices.len() < 10 {
            return 0.0;
        }

        let recent: Vec<_> = prices.iter().rev().take(VOLATILITY_WINDOW).cloned().collect();
        let n = recent.len() as f64;
        let mean = recent.iter().map(|p| *p as f64).sum::<f64>() / n;

        let variance = recent.iter()
            .map(|p| {
                let diff = *p as f64 - mean;
                diff * diff
            })
            .sum::<f64>() / n;

        let std_dev = variance.sqrt().unwrap_or(0.0);
        std_dev / mean
    }

    fn calculate_trend(&self, prices: &VecDeque<u32>) -> f64 {
        if prices.len() < 20 {
            return 0.0;
        }

        let recent: Vec<_> = prices.iter().rev().take(20).collect();
        let older: Vec<_> = prices.iter().rev().skip(20).take(20).collect();

        if recent.is_empty() || older.is_empty() {
            return 0.0;
        }

        let recent_avg = recent.iter().map(|p| *p as f64).sum::<f64>() / recent.len() as f64;
        let older_avg = older.iter().map(|p| *p as f64).sum::<f64>() / older.len() as f64;

        (recent_avg - older_avg) / older_avg
    }

    fn calculate_liquidity(&self, spreads: &VecDeque<u32>, volumes: &VecDeque<u64>) -> f64 {
        let spread_score = if spreads.len() >= 10 {
            let avg_spread = spreads.iter().sum::<u32>() as f64 / spreads.len() as f64;
            let max_acceptable_spread = 50.0;
            (1.0 - (avg_spread / max_acceptable_spread)).clamp(0.0, 1.0)
        } else {
            0.5
        };

        let volume_score = if volumes.len() >= 10 {
            let avg_volume = volumes.iter().sum::<u64>() as f64 / volumes.len() as f64;
            let min_acceptable_volume = 1000.0;
            (avg_volume / (avg_volume + min_acceptable_volume)).clamp(0.0, 1.0)
        } else {
            0.5
        };

        (spread_score + volume_score) / 2.0
    }
}

use std::collections::VecDeque;
