use crate::Db;
use chrono::{DateTime, Utc};
use common::{AppResult, Fill, MarketId, OrderId, Price, Quantity, Side, UserId};
use common::types::FillId;
use uuid::Uuid;

#[derive(sqlx::FromRow)]
pub struct FillRow {
    pub id:             Uuid,
    pub market_id:      Uuid,
    pub maker_order_id: Uuid,
    pub taker_order_id: Uuid,
    pub maker_user_id:  Uuid,
    pub taker_user_id:  Uuid,
    pub price:          i32,
    pub quantity:       i64,
    pub aggressor:      Side,
    pub sequence:       i64,
    pub filled_at:      DateTime<Utc>,
    pub transaction_id: Option<String>,
}

impl From<FillRow> for Fill {
    fn from(r: FillRow) -> Self {
        Self {
            id:             FillId(r.id),
            market_id:      MarketId(r.market_id),
            maker_order_id: OrderId(r.maker_order_id),
            taker_order_id: OrderId(r.taker_order_id),
            maker_user_id:  UserId(r.maker_user_id),
            taker_user_id:  UserId(r.taker_user_id),
            price:          Price(r.price as u32),
            quantity:       Quantity(r.quantity as u64),
            aggressor:      r.aggressor,
            sequence:       r.sequence as u64,
            filled_at:      r.filled_at,
            transaction_id: r.transaction_id,
        }
    }
}

pub enum InsertFillOutcome {
    Inserted,
    Duplicate,
}

impl Db {
    pub async fn insert_fill_dedup(&self, fill: &Fill) -> AppResult<InsertFillOutcome> {
        let result = sqlx::query(
            "INSERT INTO fills
             (id, market_id, maker_order_id, taker_order_id,
              maker_user_id, taker_user_id, price, quantity, aggressor, sequence, filled_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
             ON CONFLICT (market_id, sequence) DO NOTHING",
        )
        .bind(fill.id.0)
        .bind(fill.market_id.0)
        .bind(fill.maker_order_id.0)
        .bind(fill.taker_order_id.0)
        .bind(fill.maker_user_id.0)
        .bind(fill.taker_user_id.0)
        .bind(fill.price.0 as i32)
        .bind(fill.quantity.0 as i64)
        .bind(fill.aggressor)
        .bind(fill.sequence as i64)
        .bind(fill.filled_at)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            return Ok(InsertFillOutcome::Duplicate);
        }
        Ok(InsertFillOutcome::Inserted)
    }

    pub async fn list_fills(&self, market_id: MarketId, limit: i64) -> AppResult<Vec<Fill>> {
        let rows = sqlx::query_as::<_, FillRow>(
            "SELECT * FROM fills WHERE market_id = $1 ORDER BY sequence DESC LIMIT $2",
        )
        .bind(market_id.0)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        Ok(rows.into_iter().map(Into::into).collect())
    }
}
