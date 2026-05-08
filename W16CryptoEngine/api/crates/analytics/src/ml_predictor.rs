use chrono::{DateTime, Utc, Duration};
use common::{Fill, MarketId, OrderBook, Price, Quantity};
use polymarket::MarketDataEvent;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

const FEATURE_WINDOW: usize = 50;
const PREDICTION_HORIZON_MS: i64 = 60000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MLPrediction {
    pub market_id: String,
    pub predicted_direction: f64,
    pub confidence: f64,
    pub expected_return_bps: i32,
    pub volatility_forecast: f64,
    pub features: Vec<f64>,
    pub timestamp: DateTime<Utc>,
    pub model_version: String,
}

#[derive(Debug, Clone)]
struct MarketFeatures {
    returns: VecDeque<f64>,
    volumes: VecDeque<u64>,
    bid_depth: VecDeque<u64>,
    ask_depth: VecDeque<u64>,
    spread: VecDeque<u32>,
    trade_imbalance: VecDeque<f64>,
    volatility: VecDeque<f64>,
    price_levels: VecDeque<u32>,
}

pub struct MLPredictor {
    models: Arc<RwLock<std::collections::HashMap<MarketId, MarketFeatures>>>,
    model_weights: Vec<f64>,
}

impl MLPredictor {
    pub fn new() -> Self {
        let weights = vec![
            0.15, -0.12, 0.25, 0.08, -0.20,
            0.18, 0.05, -0.10, 0.30, 0.12,
        ];

        Self {
            models: Arc::new(RwLock::new(std::collections::HashMap::new())),
            model_weights: weights,
        }
    }

    pub async fn on_fill(&self, fill: &Fill) {
        let mut models = self.models.write().await;
        let features = models.entry(fill.market_id).or_insert_with(|| MarketFeatures {
            returns: VecDeque::with_capacity(FEATURE_WINDOW),
            volumes: VecDeque::with_capacity(FEATURE_WINDOW),
            bid_depth: VecDeque::with_capacity(FEATURE_WINDOW),
            ask_depth: VecDeque::with_capacity(FEATURE_WINDOW),
            spread: VecDeque::with_capacity(FEATURE_WINDOW),
            trade_imbalance: VecDeque::with_capacity(FEATURE_WINDOW),
            volatility: VecDeque::with_capacity(FEATURE_WINDOW),
            price_levels: VecDeque::with_capacity(FEATURE_WINDOW),
        });

        features.volumes.push_back(fill.quantity.0);
        features.price_levels.push_back(fill.price.0);

        if features.price_levels.len() >= 2 {
            let last = *features.price_levels.back().unwrap_or(&0) as f64;
            let prev = *features.price_levels.iter().nth_back(1).unwrap_or(&0) as f64;
            let ret = if prev != 0.0 { (last - prev) / prev } else { 0.0 };
            features.returns.push_back(ret);
        }

        if features.returns.len() > FEATURE_WINDOW {
            features.returns.pop_front();
        }
        if features.volumes.len() > FEATURE_WINDOW {
            features.volumes.pop_front();
        }
        if features.price_levels.len() > FEATURE_WINDOW {
            features.price_levels.pop_front();
        }
    }

    pub async fn on_book_update(&self, market_id: MarketId, book: &OrderBook) {
        let mut models = self.models.write().await;
        let features = models.entry(market_id).or_insert_with(|| MarketFeatures {
            returns: VecDeque::with_capacity(FEATURE_WINDOW),
            volumes: VecDeque::with_capacity(FEATURE_WINDOW),
            bid_depth: VecDeque::with_capacity(FEATURE_WINDOW),
            ask_depth: VecDeque::with_capacity(FEATURE_WINDOW),
            spread: VecDeque::with_capacity(FEATURE_WINDOW),
            trade_imbalance: VecDeque::with_capacity(FEATURE_WINDOW),
            volatility: VecDeque::with_capacity(FEATURE_WINDOW),
            price_levels: VecDeque::with_capacity(FEATURE_WINDOW),
        });

        let bid_qty: u64 = book.bids.iter().map(|p| p.quantity.0).sum();
        let ask_qty: u64 = book.asks.iter().map(|p| p.quantity.0).sum();

        features.bid_depth.push_back(bid_qty);
        features.ask_depth.push_back(ask_qty);

        if let (Some(best_bid), Some(best_ask)) = (
            book.bids.first().map(|p| p.price),
            book.asks.first().map(|p| p.price),
        ) {
            features.spread.push_back(best_ask.0.saturating_sub(best_bid.0));

            let imbalance = if bid_qty + ask_qty > 0 {
                (bid_qty as f64 - ask_qty as f64) / (bid_qty + ask_qty) as f64
            } else {
                0.0
            };
            features.trade_imbalance.push_back(imbalance);
        }

        if features.returns.len() >= 10 {
            let recent: Vec<_> = features.returns.iter().rev().take(10).copied().collect();
            let mean = recent.iter().sum::<f64>() / recent.len() as f64;
            let variance = recent.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / recent.len() as f64;
            features.volatility.push_back(variance.sqrt());
        }

        if features.bid_depth.len() > FEATURE_WINDOW {
            features.bid_depth.pop_front();
        }
        if features.ask_depth.len() > FEATURE_WINDOW {
            features.ask_depth.pop_front();
        }
        if features.spread.len() > FEATURE_WINDOW {
            features.spread.pop_front();
        }
        if features.trade_imbalance.len() > FEATURE_WINDOW {
            features.trade_imbalance.pop_front();
        }
        if features.volatility.len() > FEATURE_WINDOW {
            features.volatility.pop_front();
        }
    }

    pub async fn predict(&self, market_id: MarketId) -> Option<MLPrediction> {
        let models = self.models.read().await;
        let features = models.get(&market_id)?;

        if features.returns.len() < 20 {
            return None;
        }

        let feature_vector = self.extract_features(features);

        let prediction_score: f64 = feature_vector
            .iter()
            .zip(self.model_weights.iter())
            .map(|(f, w)| f * w)
            .sum();

        let confidence = self.calculate_confidence(features, prediction_score);

        let expected_return = (prediction_score * 10000.0) as i32;

        let vol_forecast = features.volatility.back().copied().unwrap_or(0.02);

        Some(MLPrediction {
            market_id: market_id.0.to_string(),
            predicted_direction: prediction_score.tanh(),
            confidence,
            expected_return_bps: expected_return,
            volatility_forecast: vol_forecast,
            features: feature_vector,
            timestamp: Utc::now(),
            model_version: "v1.0-linear".to_string(),
        })
    }

    fn extract_features(&self, f: &MarketFeatures) -> Vec<f64> {
        let mut features = vec![];

        let recent_ret: Vec<_> = f.returns.iter().rev().take(5).copied().collect();
        let momentum = if recent_ret.len() >= 5 {
            recent_ret.iter().sum::<f64>()
        } else {
            0.0
        };
        features.push(momentum);

        let mean_return = f.returns.iter().sum::<f64>() / f.returns.len().max(1) as f64;
        features.push(mean_return);

        let vol = f.volatility.back().copied().unwrap_or(0.0);
        features.push(vol);

        let depth_imbalance = if f.bid_depth.back().is_some() && f.ask_depth.back().is_some() {
            let bid = *f.bid_depth.back().unwrap_or(&0) as f64;
            let ask = *f.ask_depth.back().unwrap_or(&0) as f64;
            (bid - ask) / (bid + ask + 1.0)
        } else {
            0.0
        };
        features.push(depth_imbalance);

        let spread_norm = f.spread.back().map(|s| *s as f64 / 10000.0).unwrap_or(0.0);
        features.push(spread_norm);

        let imbalance = f.trade_imbalance.back().copied().unwrap_or(0.0);
        features.push(imbalance);

        let volume_ma: f64 = f.volumes.iter().rev().take(10).map(|v| *v as f64).sum::<f64>()
            / f.volumes.iter().rev().take(10).count().max(1) as f64;
        let recent_vol = f.volumes.back().copied().unwrap_or(0) as f64;
        let volume_spike = if volume_ma > 0.0 {
            recent_vol / volume_ma - 1.0
        } else {
            0.0
        };
        features.push(volume_spike);

        let skew = self.calculate_skew(&f.returns);
        features.push(skew);

        let kurtosis = self.calculate_kurtosis(&f.returns);
        features.push(kurtosis);

        let range = if f.price_levels.len() >= 2 {
            let max = *f.price_levels.iter().max().unwrap_or(&0) as f64;
            let min = *f.price_levels.iter().min().unwrap_or(&0) as f64;
            if min > 0.0 { (max - min) / min } else { 0.0 }
        } else {
            0.0
        };
        features.push(range);

        features
    }

    fn calculate_confidence(&self, f: &MarketFeatures, score: f64) -> f64 {
        let data_quality = (f.returns.len() as f64 / FEATURE_WINDOW as f64).min(1.0);

        let vol_stability = if f.volatility.len() >= 5 {
            let recent: Vec<_> = f.volatility.iter().rev().take(5).copied().collect();
            let mean = recent.iter().sum::<f64>() / recent.len() as f64;
            let variance = recent.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / recent.len() as f64;
            1.0 - (variance.sqrt() / mean).min(1.0)
        } else {
            0.5
        };

        let score_magnitude = score.abs().min(1.0);

        (data_quality * 0.3 + vol_stability * 0.3 + score_magnitude * 0.4).clamp(0.0, 1.0)
    }

    fn calculate_skew(&self, returns: &VecDeque<f64>) -> f64 {
        if returns.len() < 10 {
            return 0.0;
        }
        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let n = returns.len() as f64;
        let m3: f64 = returns.iter().map(|r| (r - mean).powi(3)).sum::<f64>() / n;
        let m2: f64 = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
        if m2 > 0.0 {
            m3 / m2.powf(1.5)
        } else {
            0.0
        }
    }

    fn calculate_kurtosis(&self, returns: &VecDeque<f64>) -> f64 {
        if returns.len() < 10 {
            return 0.0;
        }
        let mean = returns.iter().sum::<f64>() / returns.len() as f64;
        let n = returns.len() as f64;
        let m4: f64 = returns.iter().map(|r| (r - mean).powi(4)).sum::<f64>() / n;
        let m2: f64 = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / n;
        if m2 > 0.0 {
            m4 / m2.powi(2) - 3.0
        } else {
            0.0
        }
    }
}
