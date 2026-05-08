use async_nats::Client;
use common::{AppError, AppResult, MarketId, OrderId, Price, Quantity, Side, UserId};
use common::types::FillId;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tokio::time::timeout;
use uuid::Uuid;

#[derive(Clone)]
pub struct Nats {
    client:          Client,
    order_subject:   String,
    fill_subject:    String,
    depth_subject:   String,
    request_timeout: Duration,
}

impl Nats {
    pub async fn connect(
        nats_url: &str,
        order_subject:   String,
        fill_subject:    String,
        depth_subject:   String,
        connect_timeout: Duration,
        request_timeout: Duration,
    ) -> AppResult<Self> {
        let client: async_nats::Client = timeout(connect_timeout, async_nats::connect(nats_url))
            .await
            .map_err(|_| AppError::Messaging("nats connect timeout".into()))?
            .map_err(|e: async_nats::ConnectError| AppError::Messaging(e.to_string()))?;

        Ok(Self {
            client,
            order_subject,
            fill_subject,
            depth_subject,
            request_timeout,
        })
    }

    pub async fn publish_order(&self, payload: &EngineOrderMsg) -> AppResult<()> {
        let data = encode_order_msg(payload);
        self.client
            .publish(self.order_subject.clone(), data.into())
            .await
            .map_err(|e| AppError::Messaging(e.to_string()))
    }

    pub async fn publish_cancel(&self, payload: &EngineCancelMsg) -> AppResult<()> {
        let data = encode_cancel_msg(payload);
        self.client
            .publish(self.order_subject.clone(), data.into())
            .await
            .map_err(|e| AppError::Messaging(e.to_string()))
    }

    pub async fn subscribe_fills(
        &self,
    ) -> AppResult<async_nats::Subscriber> {
        self.client
            .subscribe(self.fill_subject.clone())
            .await
            .map_err(|e| AppError::Messaging(e.to_string()))
    }

    pub async fn subscribe_depth(
        &self,
        market_id: MarketId,
    ) -> AppResult<async_nats::Subscriber> {
        let subject = format!("{}.{}", self.depth_subject, market_id.0);
        self.client
            .subscribe(subject)
            .await
            .map_err(|e| AppError::Messaging(e.to_string()))
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub async fn is_connected(&self) -> bool {
        let _ = self.client.server_info();
        true
    }
}

#[derive(Debug, Clone)]
pub struct EngineOrderMsg {
    pub order_id:  OrderId,
    pub market_id: MarketId,
    pub user_id:   UserId,
    pub side:      Side,
    pub price:     u32,
    pub quantity:  u64,
    pub sequence:  u64,
    pub is_market: bool,
}

#[derive(Debug, Clone)]
pub struct EngineCancelMsg {
    pub order_id:  OrderId,
    pub market_id: MarketId,
    pub sequence:  u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineFillEvent {
    pub fill_id:        Uuid,
    pub market_id:      Uuid,
    pub maker_order_id: Uuid,
    pub taker_order_id: Uuid,
    pub maker_user_id:  Uuid,
    pub taker_user_id:  Uuid,
    pub price:          u32,
    pub quantity:       u64,
    pub aggressor_side: String,
    pub sequence:       u64,
    pub timestamp_ns:   u64,
}

impl EngineFillEvent {
    pub fn fill_id(&self) -> FillId {
        FillId(self.fill_id)
    }

    pub fn market_id(&self) -> MarketId {
        MarketId(self.market_id)
    }

    pub fn maker_order_id(&self) -> OrderId {
        OrderId(self.maker_order_id)
    }

    pub fn taker_order_id(&self) -> OrderId {
        OrderId(self.taker_order_id)
    }

    pub fn maker_user_id(&self) -> UserId {
        UserId(self.maker_user_id)
    }

    pub fn taker_user_id(&self) -> UserId {
        UserId(self.taker_user_id)
    }

    pub fn price(&self) -> Price {
        Price(self.price)
    }

    pub fn quantity(&self) -> Quantity {
        Quantity(self.quantity)
    }

    pub fn aggressor(&self) -> Side {
        match self.aggressor_side.as_str() {
            "buy" => Side::Buy,
            _ => Side::Sell,
        }
    }
}

pub fn decode_fill_event(data: &[u8]) -> Option<EngineFillEvent> {
    serde_json::from_slice(data).ok()
}

fn encode_order_msg(msg: &EngineOrderMsg) -> Vec<u8> {
    let mut buf = Vec::with_capacity(64);
    buf.extend_from_slice(b"\x01");
    buf.extend_from_slice(msg.order_id.0.as_bytes());
    buf.extend_from_slice(msg.market_id.0.as_bytes());
    buf.extend_from_slice(msg.user_id.0.as_bytes());
    buf.push(if msg.side == Side::Buy { 0 } else { 1 });
    buf.push(if msg.is_market { 1 } else { 0 });
    buf.extend_from_slice(&msg.price.to_le_bytes());
    buf.extend_from_slice(&msg.quantity.to_le_bytes());
    buf.extend_from_slice(&msg.sequence.to_le_bytes());
    buf
}

fn encode_cancel_msg(msg: &EngineCancelMsg) -> Vec<u8> {
    let mut buf = Vec::with_capacity(48);
    buf.extend_from_slice(b"\x02");
    buf.extend_from_slice(msg.order_id.0.as_bytes());
    buf.extend_from_slice(msg.market_id.0.as_bytes());
    buf.extend_from_slice(&msg.sequence.to_le_bytes());
    buf
}
