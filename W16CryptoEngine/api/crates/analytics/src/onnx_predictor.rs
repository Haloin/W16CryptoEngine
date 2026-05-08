use crate::ml_predictor::MLPrediction;
use common::{Fill, MarketId, OrderBook};
use ndarray::{ArrayView1};
use ort::session::Session;
use ort::value::Value;
use std::collections::VecDeque;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

const FEATURE_WINDOW: usize = 50;
const NUM_FEATURES: usize = 10;

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

pub struct OnnxPredictor {
    models: Arc<RwLock<std::collections::HashMap<MarketId, MarketFeatures>>>,
    onnx_session: Arc<RwLock<Option<Session>>>,
    use_onnx: bool,
}

impl OnnxPredictor {
    pub fn new(model_path: Option<&str>) -> Result<Self, common::AppError> {
        let onnx_session = if let Some(path) = model_path {
            match Self::load_onnx_model(path) {
                Ok(session) => {
                    info!("ONNX model loaded from {}", path);
                    Arc::new(RwLock::new(Some(session)))
                }
                Err(e) => {
                    warn!("Failed to load ONNX model: {}, falling back to linear model", e);
                    Arc::new(RwLock::new(None))
                }
            }
        } else {
            Arc::new(RwLock::new(None))
        };

        let use_onnx = true;

        Ok(Self {
            models: Arc::new(RwLock::new(std::collections::HashMap::new())),
            onnx_session,
            use_onnx,
        })
    }

    fn load_onnx_model(path: &str) -> ort::Result<Session> {
        let session = Session::builder()?
            .commit_from_file(path)?;
        Ok(session)
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

        if self.use_onnx {
            self.predict_onnx(&feature_vector, market_id).await
        } else {
            self.predict_linear(&feature_vector, features, market_id).await
        }
    }

    async fn predict_onnx(
        &self,
        features: &[f64],
        market_id: MarketId,
    ) -> Option<MLPrediction> {
        let mut session_guard = self.onnx_session.write().await;
        let session = session_guard.as_mut()?;

        let input_shape = vec![1, features.len()];
        let input_data: Vec<f32> = features.iter().map(|&x| x as f32).collect();
        let input_value = Value::from_array((input_shape, input_data)).ok()?;

        let outputs = session.run(vec![("input", input_value)]).ok()?;
        let output_tensor = outputs.get("output")?;

        let (_output_shape, output_data) = output_tensor.try_extract_tensor::<f32>().ok()?;
        
        let output_view = ArrayView1::from(&output_data);
        
        if output_view.len() < 3 {
            warn!("ONNX model output has insufficient dimensions: {}", output_view.len());
            return None;
        }
        
        let prediction_value = output_view[0] as f64;
        let confidence = output_view.get(1).copied().unwrap_or(0.5) as f64;
        let volatility = output_view.get(2).copied().unwrap_or(0.02) as f64;

        Some(MLPrediction {
            market_id: market_id.0.to_string(),
            predicted_direction: prediction_value.tanh(),
            confidence: confidence.min(1.0).max(0.0),
            expected_return_bps: (prediction_value * 10000.0) as i32,
            volatility_forecast: volatility,
            features: features.to_vec(),
            timestamp: chrono::Utc::now(),
            model_version: "v2.0-onnx".to_string(),
        })
    }

    async fn predict_linear(
        &self,
        features: &[f64],
        market_features: &MarketFeatures,
        market_id: MarketId,
    ) -> Option<MLPrediction> {
        let model_weights = vec![
            0.15, -0.12, 0.25, 0.08, -0.20,
            0.18, 0.05, -0.10, 0.30, 0.12,
        ];

        let prediction_score: f64 = features
            .iter()
            .zip(model_weights.iter())
            .map(|(f, w)| f * w)
            .sum();

        let confidence = self.calculate_confidence(market_features, prediction_score);
        let expected_return = (prediction_score * 10000.0) as i32;
        let vol_forecast = market_features.volatility.back().copied().unwrap_or(0.02);

        Some(MLPrediction {
            market_id: market_id.0.to_string(),
            predicted_direction: prediction_score.tanh(),
            confidence,
            expected_return_bps: expected_return,
            volatility_forecast: vol_forecast,
            features: features.to_vec(),
            timestamp: chrono::Utc::now(),
            model_version: "v1.0-linear".to_string(),
        })
    }

    fn extract_features(&self, f: &MarketFeatures) -> Vec<f64> {
        let mut features = Vec::with_capacity(NUM_FEATURES);

        let recent_ret: Vec<_> = f.returns.iter().rev().take(5).copied().collect();
        let momentum = if recent_ret.len() >= 5 {
            recent_ret.iter().sum::<f64>()
        } else {
            0.0
        };
        features.push(momentum);

        let mean_return = if !f.returns.is_empty() {
            f.returns.iter().sum::<f64>() / f.returns.len() as f64
        } else {
            0.0
        };
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

        let spread_ratio = if let (Some(spread), Some(price)) = (f.spread.back(), f.price_levels.back()) {
            if *price > 0 {
                *spread as f64 / *price as f64
            } else {
                0.0
            }
        } else {
            0.0
        };
        features.push(spread_ratio);

        let volume_ma = if !f.volumes.is_empty() {
            f.volumes.iter().rev().take(10).sum::<u64>() as f64 / 10.0
        } else {
            0.0
        };
        features.push(volume_ma);

        let trade_imbalance = f.trade_imbalance.back().copied().unwrap_or(0.0);
        features.push(trade_imbalance);

        let price_velocity = if f.price_levels.len() >= 3 {
            let last = *f.price_levels.back().unwrap_or(&0) as f64;
            let mid = *f.price_levels.iter().rev().nth(1).unwrap_or(&0) as f64;
            let first = *f.price_levels.iter().rev().nth(2).unwrap_or(&0) as f64;
            (last - mid) - (mid - first)
        } else {
            0.0
        };
        features.push(price_velocity);

        let volatility_regime = if f.volatility.len() >= 10 {
            let recent_vol: f64 = f.volatility.iter().rev().take(5).sum::<f64>() / 5.0;
            let older_vol: f64 = f.volatility.iter().rev().skip(5).take(5).sum::<f64>() / 5.0;
            if older_vol > 0.0 {
                recent_vol / older_vol
            } else {
                1.0
            }
        } else {
            1.0
        };
        features.push(volatility_regime);

        let liquidity_score = if f.bid_depth.back().is_some() && f.ask_depth.back().is_some() {
            let bid = *f.bid_depth.back().unwrap_or(&0) as f64;
            let ask = *f.ask_depth.back().unwrap_or(&0) as f64;
            (bid.min(ask)).ln_1p()
        } else {
            0.0
        };
        features.push(liquidity_score);

        let trend_strength = if f.returns.len() >= 10 {
            let ups = f.returns.iter().filter(|&&r| r > 0.0).count() as f64;
            let downs = f.returns.iter().filter(|&&r| r < 0.0).count() as f64;
            if ups + downs > 0.0 {
                (ups - downs) / (ups + downs)
            } else {
                0.0
            }
        } else {
            0.0
        };
        features.push(trend_strength);

        features
    }

    fn calculate_confidence(&self, f: &MarketFeatures, prediction_score: f64) -> f64 {
        let data_quality = (f.returns.len() as f64 / FEATURE_WINDOW as f64).min(1.0);
        let prediction_magnitude = prediction_score.abs().min(1.0);
        let volatility_factor = if let Some(vol) = f.volatility.back() {
            (1.0 - (*vol * 10.0).min(0.5)).max(0.5)
        } else {
            0.8
        };

        let confidence = (data_quality * 0.3 + prediction_magnitude * 0.4 + volatility_factor * 0.3)
            .min(1.0)
            .max(0.0);

        confidence
    }

    pub fn new_linear_fallback() -> Self {
        Self {
            models: Arc::new(RwLock::new(std::collections::HashMap::new())),
            onnx_session: Arc::new(RwLock::new(None)),
            use_onnx: false,
        }
    }
}
