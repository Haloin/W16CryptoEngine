use crate::Db;
use common::{AppError, AppResult, UserId};

#[derive(sqlx::FromRow, Default, Clone)]
pub struct BalanceRow {
    pub available: i64,
    pub reserved:  i64,
}

impl Db {
    pub async fn get_balance(&self, user_id: UserId) -> AppResult<BalanceRow> {
        Ok(
            sqlx::query_as::<_, BalanceRow>(
                "SELECT available, reserved FROM balances WHERE user_id = $1",
            )
            .bind(user_id.0)
            .fetch_optional(&self.pool)
            .await?
            .unwrap_or_default(),
        )
    }

    pub async fn credit_balance(&self, user_id: UserId, amount: i64) -> AppResult<()> {
        sqlx::query(
            "INSERT INTO balances (user_id, available) VALUES ($1, $2)
             ON CONFLICT (user_id) DO UPDATE
             SET available = balances.available + $2,
                 version   = balances.version + 1",
        )
        .bind(user_id.0)
        .bind(amount)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn reserve_balance(&self, user_id: UserId, amount: i64) -> AppResult<()> {
        let affected = sqlx::query(
            "UPDATE balances
             SET available = available - $2,
                 reserved  = reserved  + $2,
                 version   = version   + 1
             WHERE user_id = $1 AND available >= $2",
        )
        .bind(user_id.0)
        .bind(amount)
        .execute(&self.pool)
        .await?
        .rows_affected();

        if affected == 0 {
            let bal = self.get_balance(user_id).await?;
            return Err(AppError::InsufficientBalance {
                needed:    amount as u64,
                available: bal.available as u64,
            });
        }
        Ok(())
    }

    pub async fn release_reservation(&self, user_id: UserId, amount: i64) -> AppResult<()> {
        sqlx::query(
            "UPDATE balances
             SET available = available + $2,
                 reserved  = GREATEST(0, reserved - $2),
                 version   = version   + 1
             WHERE user_id = $1",
        )
        .bind(user_id.0)
        .bind(amount)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn apply_fill_balances(
        &self,
        buyer_id:  UserId,
        seller_id: UserId,
        cost:      i64,
    ) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "UPDATE balances
             SET reserved = GREATEST(0, reserved - $2),
                 version  = version + 1
             WHERE user_id = $1",
        )
        .bind(buyer_id.0)
        .bind(cost)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "UPDATE balances
             SET reserved  = GREATEST(0, reserved - $2),
                 available = available + $2,
                 version   = version   + 1
             WHERE user_id = $1",
        )
        .bind(seller_id.0)
        .bind(cost)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn apply_settlement_payouts(
        &self,
        market_id: common::MarketId,
        outcome:   common::Outcome,
        payouts:   &[(UserId, i64)],
    ) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "UPDATE markets
             SET status     = 'settled',
                 outcome    = $2,
                 settled_at = now()
             WHERE id = $1",
        )
        .bind(market_id.0)
        .bind(outcome)
        .execute(&mut *tx)
        .await?;

        for (user_id, amount) in payouts {
            sqlx::query(
                "INSERT INTO balances (user_id, available) VALUES ($1, $2)
                 ON CONFLICT (user_id) DO UPDATE
                 SET available = balances.available + $2,
                     version   = balances.version   + 1",
            )
            .bind(user_id.0)
            .bind(amount)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }

    pub async fn record_deposit(
        &self,
        user_id:   UserId,
        amount:    i64,
        reference: &str,
    ) -> AppResult<()> {
        let mut tx = self.pool.begin().await?;

        sqlx::query(
            "INSERT INTO deposits (user_id, amount, reference) VALUES ($1, $2, $3)",
        )
        .bind(user_id.0)
        .bind(amount)
        .bind(reference)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO balances (user_id, available) VALUES ($1, $2)
             ON CONFLICT (user_id) DO UPDATE
             SET available = balances.available + $2,
                 version   = balances.version   + 1",
        )
        .bind(user_id.0)
        .bind(amount)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }
}
