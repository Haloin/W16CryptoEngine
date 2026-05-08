use crate::Db;
use chrono::{DateTime, Utc};
use common::{AppResult, FillId, MarketId, UserId};
use uuid::Uuid;

#[derive(sqlx::FromRow)]
pub struct PositionRow {
    pub user_id:             Uuid,
    pub market_id:           Uuid,
    pub net_quantity:        i64,
    pub average_entry_price: f64,
    pub realized_pnl:        f64,
    pub unrealized_pnl:    f64,
    pub total_traded_volume: i64,
    pub last_update:         DateTime<Utc>,
}

impl Db {
    pub async fn record_position_change(
        &self,
        fill_id:   FillId,
        user_id:   UserId,
        market_id: MarketId,
        delta:     i64,
    ) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO position_changes (fill_id, user_id, market_id, delta)
             VALUES ($1, $2, $3, $4)",
        )
        .bind(fill_id.0)
        .bind(user_id.0)
        .bind(market_id.0)
        .bind(delta)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_user_positions(&self, market_id: MarketId) -> AppResult<Vec<PositionRow>> {
        sqlx::query_as::<_, PositionRow>(
            "SELECT user_id, market_id, SUM(delta) as net_quantity
             FROM position_changes
             WHERE market_id = $1
             GROUP BY user_id, market_id
             HAVING SUM(delta) != 0",
        )
        .bind(market_id.0)
        .fetch_all(&self.pool)
        .await
        .map_err(Into::into)
    }

    pub async fn get_position_for_user(
        &self,
        user_id:   UserId,
        market_id: MarketId,
    ) -> AppResult<i64> {
        let net: Option<i64> = sqlx::query_scalar(
            "SELECT SUM(delta)
             FROM position_changes
             WHERE user_id = $1 AND market_id = $2",
        )
        .bind(user_id.0)
        .bind(market_id.0)
        .fetch_one(&self.pool)
        .await?;
        Ok(net.unwrap_or(0))
    }
}
