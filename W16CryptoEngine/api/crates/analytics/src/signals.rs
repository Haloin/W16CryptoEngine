use chrono::{DateTime, Utc, Duration};
use common::{Fill, MarketId, OrderBook, Price, Quantity, Side};
use common::types::{Signal, SignalId, SignalType};
use polymarket::MarketDataEvent;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, error, info};

const WINDOW_SIZE: usize = 100;
const IMBALANCE_THRESHOLD: f64 = 0.6;
const MOMENTUM_THRESHOLD: f64 = 0.02;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TradingSignalType {
    Buy,
    Sell,
    Hold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalStrength {
    Strong,
    Moderate,
    Weak,
}

pub fn create_signal(
    market_id: &str,
    signal_type: TradingSignalType,
    strength: SignalStrength,
    confidence: f64,
    price_target: Option<u32>,
    stop_loss: Option<u32>,
    indicators: Vec<String>,
) -> Signal {
    Signal {
        id: SignalId::new(),
        market_id: MarketId(uuid::Uuid::parse_str(market_id).unwrap_or_else(|_| uuid::Uuid::new_v4())),
        signal_type: match signal_type {
            TradingSignalType::Buy => CommonSignalType::Buy,
            TradingSignalType::Sell => CommonSignalType::Sell,
            TradingSignalType::Hold => CommonSignalType::Hold,
        },
        strength: confidence,
        confidence,
        expected_return: price_target.unwrap_or(0) as i64,
        time_horizon: 3600,
        created_at: Utc::now(),
        expires_at: Utc::now() + chrono::Duration::hours(1),
    }
}

struct MarketData {
    fills: VecDeque<FillData>,
    bids: Vec<(u32, u64)>,
    asks: Vec<(u32, u64)>,
    last_update: DateTime<Utc>,
}

struct FillData {
    price: u32,
    quantity: Quantity,
    side: Side,
    timestamp: DateTime<Utc>,
}

pub struct SignalGenerator {
    data: Arc<RwLock<HashMap<MarketId, MarketData>>>,
}

impl SignalGenerator {
    pub fn new() -> Self {
        Self {
            data: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn on_fill(&self, fill: &Fill) {
        let mut data = self.data.write().await;
        let market_data = data.entry(fill.market_id).or_insert_with(|| MarketData {
            fills: VecDeque::with_capacity(WINDOW_SIZE),
            bids: vec![],
            asks: vec![],
            last_update: Utc::now(),
        });

        market_data.fills.push_back(FillData {
            price: fill.price.0,
            quantity: fill.quantity,
            side: fill.aggressor,
            timestamp: Utc::now(),
        });

        if market_data.fills.len() > WINDOW_SIZE {
            market_data.fills.pop_front();
        }

        market_data.last_update = Utc::now();
    }

    pub async fn on_book_update(&self, market_id: MarketId, book: &OrderBook) {
        let mut data = self.data.write().await;
        let market_data = data.entry(market_id).or_insert_with(|| MarketData {
            fills: VecDeque::with_capacity(WINDOW_SIZE),
            bids: vec![],
            asks: vec![],
            last_update: Utc::now(),
        });

        market_data.bids = book.bids.iter().map(|p| (p.price.0, p.quantity.0)).collect();
        market_data.asks = book.asks.iter().map(|p| (p.price.0, p.quantity.0)).collect();
        market_data.last_update = Utc::now();
    }

    pub async fn generate(&self, market_id: MarketId) -> Vec<Signal> {
        let data = self.data.read().await;
        let market_data = match data.get(&market_id) {
            Some(d) => d,
            None => return vec![],
        };

        let mut signals = vec![];
        let market_id_str = market_id.0.to_string();

        if let Some(imbalance_signal) = self.check_order_flow_imbalance(&market_data, &market_id_str) {
            signals.push(imbalance_signal);
        }

        if let Some(momentum_signal) = self.check_momentum(&market_data, &market_id_str) {
            signals.push(momentum_signal);
        }

        if let Some(support_resist_signal) = self.check_support_resistance(&market_data, &market_id_str) {
            signals.push(support_resist_signal);
        }

        signals
    }

    fn check_order_flow_imbalance(&self, data: &MarketData, market_id: &str) -> Option<Signal> {
        let recent_fills: Vec<_> = data.fills.iter().rev().take(20).collect();
        if recent_fills.len() < 10 {
            return None;
        }

        let buy_volume: u64 = recent_fills.iter().filter(|f| matches!(f.side, Side::Buy)).map(|f| f.quantity.0).sum();
        let sell_volume: u64 = recent_fills.iter().filter(|f| matches!(f.side, Side::Sell)).map(|f| f.quantity.0).sum();
        let total_volume = buy_volume + sell_volume;

        if total_volume == 0 {
            return None;
        }

        let buy_ratio = buy_volume as f64 / total_volume as f64;
        let sell_ratio = sell_volume as f64 / total_volume as f64;

        if buy_ratio > IMBALANCE_THRESHOLD {
            let confidence = (buy_ratio - 0.5) * 2.0;
            return Some(create_signal(
                market_id,
                TradingSignalType::Buy,
                self.confidence_to_strength(confidence),
                confidence,
                None,
                None,
                vec!["order_flow_imbalance".to_string()],
            ));
        }

        if sell_ratio > IMBALANCE_THRESHOLD {
            let confidence = (sell_ratio - 0.5) * 2.0;
            return Some(create_signal(
                market_id,
                TradingSignalType::Sell,
                self.confidence_to_strength(confidence),
                confidence,
                None,
                None,
                vec!["order_flow_imbalance".to_string()],
            ));
        }

        None
    }

    fn check_momentum(&self, data: &MarketData, market_id: &str) -> Option<Signal> {
        if data.fills.len() < 20 {
            return None;
        }

        let recent: Vec<_> = data.fills.iter().rev().take(10).collect();
        let older: Vec<_> = data.fills.iter().rev().skip(10).take(10).collect();

        if recent.is_empty() || older.is_empty() {
            return None;
        }

        let recent_avg = recent.iter().map(|f| f.price).sum::<u32>() as f64 / recent.len() as f64;
        let older_avg = older.iter().map(|f| f.price).sum::<u32>() as f64 / older.len() as f64;

        let change = (recent_avg - older_avg) / older_avg;

        if change.abs() > MOMENTUM_THRESHOLD {
            let (signal_type, confidence) = if change > 0.0 {
                (CommonSignalType::Buy, change.min(1.0))
            } else {
                (CommonSignalType::Sell, change.abs().min(1.0))
            };

            return Some(create_signal(
                market_id,
                match signal_type {
                    SignalType::Buy => TradingSignalType::Buy,
                    SignalType::Sell => TradingSignalType::Sell,
                    SignalType::Hold => TradingSignalType::Hold,
                    SignalType::Close => TradingSignalType::Hold,
                },
                self.confidence_to_strength(confidence),
                confidence,
                Some((recent_avg * (1.0 + change)) as u32),
                Some((recent_avg * (1.0 - change)) as u32),
                vec!["price_momentum".to_string()],
            ));
        }

        None
    }

    fn check_support_resistance(&self, data: &MarketData, market_id: &str) -> Option<Signal> {
        if data.bids.is_empty() || data.asks.is_empty() {
            return None;
        }

        let best_bid = data.bids.first()?.0;
        let best_ask = data.asks.first()?.0;
        let spread = best_ask.saturating_sub(best_bid);

        let total_bid_qty: u64 = data.bids.iter().map(|(_, q)| q).sum();
        let total_ask_qty: u64 = data.asks.iter().map(|(_, q)| q).sum();

        if total_bid_qty == 0 || total_ask_qty == 0 {
            return None;
        }

        let bid_pressure = total_bid_qty as f64 / (total_bid_qty + total_ask_qty) as f64;
        let ask_pressure = total_ask_qty as f64 / (total_bid_qty + total_ask_qty) as f64;

        if bid_pressure > 0.65 && spread < 10 {
            return Some(create_signal(
                market_id,
                TradingSignalType::Buy,
                SignalStrength::Moderate,
                bid_pressure,
                Some(best_ask),
                Some(best_bid.saturating_sub(20)),
                vec!["support_wall".to_string()],
            ));
        }

        if ask_pressure > 0.65 && spread < 10 {
            return Some(create_signal(
                market_id,
                TradingSignalType::Sell,
                SignalStrength::Moderate,
                ask_pressure,
                Some(best_bid),
                Some(best_ask.saturating_add(20)),
                vec!["resistance_wall".to_string()],
            ));
        }

        None
    }

    fn confidence_to_strength(&self, confidence: f64) -> SignalStrength {
        if confidence > 0.8 {
            SignalStrength::Strong
        } else if confidence > 0.5 {
            SignalStrength::Moderate
        } else {
            SignalStrength::Weak
        }
    }
}

