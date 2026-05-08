use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Market {
    pub id: String,
    pub slug: String,
    pub title: String,
    pub description: String,
    pub outcomes: Vec<Outcome>,
    pub active: bool,
    pub closed: bool,
    pub archived: bool,
    pub end_date: DateTime<Utc>,
    pub liquidity: f64,
    pub volume: f64,
    pub category: String,
    pub resolution: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Outcome {
    pub id: String,
    pub name: String,
    pub price: f64,
    pub probability: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    pub market_id: String,
    pub asset_id: String,
    pub bids: Vec<OrderBookLevel>,
    pub asks: Vec<OrderBookLevel>,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookLevel {
    pub price: f64,
    pub size: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trade {
    pub id: String,
    pub market_id: String,
    pub asset_id: String,
    pub price: f64,
    pub size: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    pub available: u64,
    pub reserved: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserProfile {
    pub user_id: String,
    pub email: String,
    pub created_at: DateTime<Utc>,
    pub last_login: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MakerOrder {
    pub order_id: String,
    pub size_matched: f64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Order {
    pub id: String,
    pub market_id: String,
    pub asset_id: String,
    pub price: f64,
    pub size: f64,
    pub side: Side,
    pub status: OrderStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum OrderStatus {
    Open,
    Filled,
    Cancelled,
    Live,
    PartiallyFilled,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrderRequest {
    pub market_id: String,
    pub asset_id: String,
    pub price: f64,
    pub size: f64,
    pub side: Side,
    pub taker_fee: Option<f64>,
    pub maker_fee: Option<f64>,
    pub nonce: u64,
    pub self_trade_behavior: Option<String>,
    pub time_in_force: String,
    pub expiration: Option<u64>,
    pub neg_risk: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateOrderResponse {
    pub order_id: String,
    pub status: OrderStatus,
    pub taking_fee: f64,
    pub making_fee: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CancelOrderRequest {
    pub order_id: String,
    pub market_id: String,
    pub asset_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebSocketMessage {
    pub event_type: String,
    pub payload: serde_json::Value,
    pub timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum MarketDataEvent {
    #[serde(rename = "orderbook")]
    OrderBook {
        market_id: String,
        asset_id: String,
        bids: Vec<OrderBookLevel>,
        asks: Vec<OrderBookLevel>,
        timestamp: DateTime<Utc>,
    },
    #[serde(rename = "trade")]
    Trade {
        trade: Trade,
    },
    #[serde(rename = "price_change")]
    PriceChange {
        market_id: String,
        asset_id: String,
        price: f64,
        change_24h: f64,
    },
}

#[derive(Debug, Clone)]
pub struct ClobRewards {
    pub market_id: String,
    pub asset_id: String,
    pub rewards_daily_rate: u64,
    pub rewards_daily_rate_per_taker_order: Option<u64>,
}
