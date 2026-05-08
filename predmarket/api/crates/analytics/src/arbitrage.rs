use chrono::{DateTime, Utc, Duration};
use common::{AppResult, Fill, MarketId, Price};
use polymarket::MarketDataEvent;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn, debug};

const LOOKBACK_WINDOW: usize = 200;
const ZSCORE_THRESHOLD: f64 = 2.0;
const CORRELATION_THRESHOLD: f64 = 0.85;
const MIN_PROFIT_BPS: i32 = 10;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArbitrageOpportunity {
    pub market_a: String,
    pub market_b: String,
    pub spread_zscore: f64,
    pub correlation: f64,
    pub hedge_ratio: f64,
    pub suggested_action: ArbAction,
    pub expected_profit_bps: i32,
    pub confidence: f64,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArbAction {
    BuyA_SellB,
    SellA_BuyB,
    Hold,
}

#[derive(Debug, Clone)]
struct MarketPair {
    market_a: MarketId,
    market_b: MarketId,
    prices_a: VecDeque<u32>,
    prices_b: VecDeque<u32>,
    spreads: VecDeque<f64>,
    correlation: f64,
    hedge_ratio: f64,
    spread_mean: f64,
    spread_std: f64,
    last_update: DateTime<Utc>,
}

pub struct ArbitrageDetector {
    pairs: Arc<RwLock<std::collections::HashMap<(MarketId, MarketId), MarketPair>>>,
    active_opportunities: Arc<RwLock<Vec<ArbitrageOpportunity>>>,
}

impl ArbitrageDetector {
    pub fn new() -> Self {
        Self {
            pairs: Arc::new(RwLock::new(std::collections::HashMap::new())),
            active_opportunities: Arc::new(RwLock::new(vec![])),
        }
    }

    pub async fn register_pair(&self, market_a: MarketId, market_b: MarketId) {
        let mut pairs = self.pairs.write().await;
        let key = if market_a.0 < market_b.0 {
            (market_a, market_b)
        } else {
            (market_b, market_a)
        };

        pairs.entry(key).or_insert_with(|| MarketPair {
            market_a: key.0,
            market_b: key.1,
            prices_a: VecDeque::with_capacity(LOOKBACK_WINDOW),
            prices_b: VecDeque::with_capacity(LOOKBACK_WINDOW),
            spreads: VecDeque::with_capacity(LOOKBACK_WINDOW),
            correlation: 0.0,
            hedge_ratio: 1.0,
            spread_mean: 0.0,
            spread_std: 0.0,
            last_update: Utc::now(),
        });
    }

    pub async fn on_fill(&self, fill: &Fill) {
        let mut pairs = self.pairs.write().await;

        for (_, pair) in pairs.iter_mut() {
            if pair.market_a == fill.market_id {
                pair.prices_a.push_back(fill.price.0);
                if pair.prices_a.len() > LOOKBACK_WINDOW {
                    pair.prices_a.pop_front();
                }
            }
            if pair.market_b == fill.market_id {
                pair.prices_b.push_back(fill.price.0);
                if pair.prices_b.len() > LOOKBACK_WINDOW {
                    pair.prices_b.pop_front();
                }
            }

            if pair.prices_a.len() >= LOOKBACK_WINDOW && pair.prices_b.len() >= LOOKBACK_WINDOW {
                self.update_pair_stats(pair);
            }

            pair.last_update = Utc::now();
        }
    }

    pub async fn on_price_update(&self, market_id: MarketId, price: u32) {
        let mut pairs = self.pairs.write().await;

        for (_, pair) in pairs.iter_mut() {
            if pair.market_a == market_id {
                pair.prices_a.push_back(price);
                if pair.prices_a.len() > LOOKBACK_WINDOW {
                    pair.prices_a.pop_front();
                }
            }
            if pair.market_b == market_id {
                pair.prices_b.push_back(price);
                if pair.prices_b.len() > LOOKBACK_WINDOW {
                    pair.prices_b.pop_front();
                }
            }

            if pair.prices_a.len() >= LOOKBACK_WINDOW && pair.prices_b.len() >= LOOKBACK_WINDOW {
                self.update_pair_stats(pair);
            }

            pair.last_update = Utc::now();
        }
    }

    fn update_pair_stats(&self, pair: &mut MarketPair) {
        let min_len = pair.prices_a.len().min(pair.prices_b.len());
        if min_len < 20 {
            return;
        }

        let prices_a: Vec<f64> = pair.prices_a.iter().rev().take(min_len).map(|p| *p as f64).collect();
        let prices_b: Vec<f64> = pair.prices_b.iter().rev().take(min_len).map(|p| *p as f64).collect();

        let mean_a: f64 = prices_a.iter().sum::<f64>() / prices_a.len() as f64;
        let mean_b: f64 = prices_b.iter().sum::<f64>() / prices_b.len() as f64;

        let std_a = self.calculate_std(&prices_a, mean_a);
        let std_b = self.calculate_std(&prices_b, mean_b);

        if std_a > 0.0 && std_b > 0.0 {
            let mut covariance = 0.0;
            for i in 0..prices_a.len() {
                covariance += (prices_a[i] - mean_a) * (prices_b[i] - mean_b);
            }
            covariance /= prices_a.len() as f64;

            pair.correlation = covariance / (std_a * std_b);
            pair.correlation = pair.correlation.clamp(-1.0, 1.0);

            if pair.correlation.abs() > 0.5 {
                let beta = if mean_b.abs() > 0.0 {
                    (mean_a / mean_b) * pair.correlation
                } else {
                    1.0
                };
                pair.hedge_ratio = beta.clamp(0.1, 10.0);
            }
        }

        let recent_a: Vec<f64> = pair.prices_a.iter().rev().take(50).map(|&p| p as f64).collect();
        let recent_b: Vec<f64> = pair.prices_b.iter().rev().take(50).map(|&p| p as f64).collect();

        if recent_a.len() == recent_b.len() {
            for i in 0..recent_a.len() {
                let spread = recent_a[i] - pair.hedge_ratio * recent_b[i];
                pair.spreads.push_back(spread);
            }

            while pair.spreads.len() > LOOKBACK_WINDOW {
                pair.spreads.pop_front();
            }

            if pair.spreads.len() >= 50 {
                pair.spread_mean = pair.spreads.iter().sum::<f64>() / pair.spreads.len() as f64;
                pair.spread_std = self.calculate_std(&pair.spreads.iter().copied().collect::<Vec<_>>(), pair.spread_mean);
            }
        }
    }

    pub async fn detect_opportunities(&self) -> Vec<ArbitrageOpportunity> {
        let pairs = self.pairs.read().await;
        let mut opportunities = vec![];
        let now = Utc::now();

        for (_, pair) in pairs.iter() {
            if pair.correlation.abs() < CORRELATION_THRESHOLD {
                continue;
            }

            if pair.spreads.is_empty() || pair.spread_std == 0.0 {
                continue;
            }

            let current_spread = *pair.spreads.back().unwrap_or(&0);
            let zscore = if pair.spread_std != 0.0 {
                (current_spread - pair.spread_mean) / pair.spread_std
            } else {
                0.0
            };

            if zscore.abs() > ZSCORE_THRESHOLD {
                let action = if zscore > 0.0 {
                    ArbAction::SellA_BuyB
                } else {
                    ArbAction::BuyA_SellB
                };

                let expected_profit = ((zscore.abs() * pair.spread_std / pair.spread_mean.abs()) * 10000.0) as i32;

                if expected_profit > MIN_PROFIT_BPS {
                    let confidence = (zscore.abs() / 3.0).min(1.0) * pair.correlation.abs();

                    opportunities.push(ArbitrageOpportunity {
                        market_a: pair.market_a.0.to_string(),
                        market_b: pair.market_b.0.to_string(),
                        spread_zscore: zscore,
                        correlation: pair.correlation,
                        hedge_ratio: pair.hedge_ratio,
                        suggested_action: action,
                        expected_profit_bps: expected_profit,
                        confidence,
                        timestamp: now,
                    });
                }
            }
        }

        let mut active = self.active_opportunities.write().await;
        *active = opportunities.clone();

        opportunities
    }

    pub async fn get_active_opportunities(&self) -> Vec<ArbitrageOpportunity> {
        let active = self.active_opportunities.read().await;
        active.clone()
    }

    pub async fn get_pair_stats(&self, market_a: MarketId, market_b: MarketId) -> Option<(f64, f64, f64)> {
        let pairs = self.pairs.read().await;
        let key = if market_a.0 < market_b.0 {
            (market_a, market_b)
        } else {
            (market_b, market_a)
        };

        pairs.get(&key).map(|p| (p.correlation, p.hedge_ratio, p.spread_std))
    }

    fn calculate_std(&self, data: &[f64], mean: f64) -> f64 {
        if data.len() < 2 {
            return 0.0;
        }

        let variance: f64 = data.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / data.len() as f64;
        variance.sqrt()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrossMarketPosition {
    pub opportunities: Vec<ArbitrageOpportunity>,
    pub total_exposure_a: i64,
    pub total_exposure_b: i64,
    pub unrealized_pnl_bps: i32,
}

pub struct CrossMarketArbitrageEngine {
    detector: Arc<ArbitrageDetector>,
    positions: Arc<RwLock<std::collections::HashMap<(MarketId, MarketId), CrossMarketPosition>>>,
}

impl CrossMarketArbitrageEngine {
    pub fn new(detector: Arc<ArbitrageDetector>) -> Self {
        Self {
            detector,
            positions: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    pub async fn scan_and_trade(&self) -> Vec<ArbitrageOpportunity> {
        let opportunities = self.detector.detect_opportunities().await;

        for opp in &opportunities {
            info!(
                market_a = %opp.market_a,
                market_b = %opp.market_b,
                zscore = %opp.spread_zscore,
                profit_bps = %opp.expected_profit_bps,
                action = ?opp.suggested_action,
                "Arbitrage opportunity detected"
            );
        }

        opportunities
    }

    pub async fn execute_spread_trade(&self, opp: &ArbitrageOpportunity, qty_a: u64, qty_b: u64) -> AppResult<()> {
        info!(
            market_a = %opp.market_a,
            market_b = %opp.market_b,
            qty_a = %qty_a,
            qty_b = %qty_b,
            "Executing spread trade"
        );

        Ok(())
    }
}
