use crate::Db;
use chrono::{DateTime, Utc};
use common::{
    AppError, AppResult, MarketId, Order, OrderId, OrderKind, OrderStatus, Price, Quantity, Side,
    UserId,
};
use uuid::Uuid;

#[derive(sqlx::FromRow)]
pub struct OrderRow {
    pub id:              Uuid,
    pub market_id:       Uuid,
    pub user_id:         Uuid,
    pub side:            Side,
    pub kind:            OrderKind,
    pub price:           Option<i32>,
    pub quantity:        i64,
    pub filled_quantity: i64,
    pub status:          OrderStatus,
    pub sequence:        i64,
    pub created_at:      DateTime<Utc>,
    pub updated_at:      DateTime<Utc>,
}

impl From<OrderRow> for Order {
    fn from(r: OrderRow) -> Self {
        Self {
            id:              OrderId(r.id),
            market_id:       MarketId(r.market_id),
            user_id:         UserId(r.user_id),
            side:            r.side,
            kind:            r.kind,
            price:           r.price.map(|p| Price(p as u32)),
            quantity:        Quantity(r.quantity as u64),
            filled_quantity: Quantity(r.filled_quantity as u64),
            status:          r.status,
            sequence:        r.sequence as u64,
            created_at:      r.created_at,
            updated_at:      r.updated_at,
        }
    }
}

impl Db {
    pub async fn insert_order(&self, order: &Order) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO orders
             (id, market_id, user_id, side, kind, price, quantity,
              filled_quantity, status, sequence, created_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
        )
        .bind(order.id.0)
        .bind(order.market_id.0)
        .bind(order.user_id.0)
        .bind(order.side)
        .bind(order.kind)
        .bind(order.price.map(|p| p.0 as i32))
        .bind(order.quantity.0 as i64)
        .bind(order.filled_quantity.0 as i64)
        .bind(order.status)
        .bind(order.sequence as i64)
        .bind(order.created_at)
        .bind(order.updated_at)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_order(&self, id: OrderId) -> AppResult<Order> {
        sqlx::query_as::<_, OrderRow>("SELECT * FROM orders WHERE id = $1")
            .bind(id.0)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::OrderNotFound(id.to_string()))
            .map(Into::into)
    }

    pub async fn update_order_fill(&self, id: OrderId, status: OrderStatus, filled: Quantity) -> AppResult<()> {
        sqlx::query(
            "UPDATE orders
             SET status          = $2,
                 filled_quantity = $3,
                 updated_at      = now()
             WHERE id = $1",
        )
        .bind(id.0)
        .bind(status)
        .bind(filled.0 as i64)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn cancel_order(&self, id: OrderId, user_id: UserId) -> AppResult<Order> {
        sqlx::query_as::<_, OrderRow>(
            "UPDATE orders
             SET status     = 'cancelled',
                 updated_at = now()
             WHERE id = $1 AND user_id = $2 AND status IN ('open', 'partial')
             RETURNING *",
        )
        .bind(id.0)
        .bind(user_id.0)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| AppError::OrderNotFound(id.to_string()))
        .map(Into::into)
    }

    pub async fn place_order_atomic(
        &self,
        order:       &Order,
        reserve_cost: i64,
    ) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;

        let market_status: Option<String> = sqlx::query_scalar(
            "SELECT status::text FROM markets WHERE id = $1 FOR SHARE",
        )
        .bind(order.market_id.0)
        .fetch_optional(&mut *tx)
        .await?;

        match market_status.as_deref() {
            Some("open") => {}
            Some(_) => {
                tx.rollback().await?;
                return Err(AppError::MarketNotOpen);
            }
            None => {
                tx.rollback().await?;
                return Err(AppError::MarketNotFound(order.market_id.to_string()));
            }
        }

        let affected = sqlx::query(
            "UPDATE balances
             SET available = available - $2,
                 reserved  = reserved  + $2,
                 version   = version   + 1
             WHERE user_id = $1 AND available >= $2",
        )
        .bind(order.user_id.0)
        .bind(reserve_cost)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if affected == 0 {
            tx.rollback().await?;
            let bal = self.get_balance(order.user_id).await?;
            return Err(AppError::InsufficientBalance {
                needed:    reserve_cost as u64,
                available: bal.available as u64,
            });
        }

        sqlx::query(
            "INSERT INTO orders
             (id, market_id, user_id, side, kind, price, quantity,
              filled_quantity, status, sequence, created_at, updated_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
        )
        .bind(order.id.0)
        .bind(order.market_id.0)
        .bind(order.user_id.0)
        .bind(order.side)
        .bind(order.kind)
        .bind(order.price.map(|p| p.0 as i32))
        .bind(order.quantity.0 as i64)
        .bind(order.filled_quantity.0 as i64)
        .bind(order.status)
        .bind(order.sequence as i64)
        .bind(order.created_at)
        .bind(order.updated_at)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn list_user_orders(
        &self,
        user_id:   UserId,
        market_id: Option<MarketId>,
        limit:     i64,
    ) -> AppResult<Vec<Order>> {
        let rows = match market_id {
            Some(mid) => {
                sqlx::query_as::<_, OrderRow>(
                    "SELECT * FROM orders
                     WHERE user_id = $1 AND market_id = $2
                     ORDER BY created_at DESC LIMIT $3",
                )
                .bind(user_id.0)
                .bind(mid.0)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, OrderRow>(
                    "SELECT * FROM orders WHERE user_id = $1 ORDER BY created_at DESC LIMIT $2",
                )
                .bind(user_id.0)
                .bind(limit)
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(rows.into_iter().map(Into::into).collect())
    }
}
