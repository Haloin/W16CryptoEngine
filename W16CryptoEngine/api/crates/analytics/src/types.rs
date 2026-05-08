use common::{MarketId, OrderId, UserId, Side, Price, Quantity};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub id: Uuid,
    pub user_id: UserId,
    pub market_id: MarketId,
    pub side: Side,
    pub entry_price: Price,
    pub current_price: Price,
    pub quantity: Quantity,
    pub realized_pnl: i64,
    pub unrealized_pnl: i64,
    pub opened_at: DateTime<Utc>,
    pub last_updated: DateTime<Utc>,
}

impl Position {
    pub fn market_value(&self) -> i64 {
        self.current_price.0 as i64 * self.quantity.0 as i64
    }

    pub fn total_pnl(&self) -> i64 {
        self.realized_pnl + self.unrealized_pnl
    }
}
