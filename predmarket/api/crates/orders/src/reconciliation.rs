use std::sync::Arc;
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use db::Db;
use messaging::EngineFillEvent;
use common::{AppError, FillId, MarketId, UserId};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingOrder {
    pub order_id: String,
    pub market_id: String,
    pub quantity: f64,
    pub timestamp: i64,
}

pub struct OrderReconciler {
    pending: Arc<DashMap<String, PendingOrder>>,
}

impl OrderReconciler {
    pub fn new() -> Self {
        Self {
            pending: Arc::new(DashMap::new()),
        }
    }

    pub async fn register(&self, order: PendingOrder) {
        self.pending.insert(order.order_id.clone(), order);
    }

    pub async fn finalize(&self, order_id: &str) {
        self.pending.remove(order_id);
    }

    pub async fn process_fill(&self, event: EngineFillEvent, db: &Db) -> Result<(), AppError> {
        let delta = if event.aggressor_side == "buy" {
            event.quantity as i64
        } else {
            -(event.quantity as i64)
        };

        db.record_position_change(
            FillId(event.fill_id),
            UserId(event.taker_user_id),
            MarketId(event.market_id),
            delta
        ).await?;

        self.finalize(&event.taker_order_id.to_string()).await;
        Ok(())
    }
}