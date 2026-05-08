use crate::Db;
use common::{AppError, AppResult, UserId};
use chrono::{DateTime, Utc};
use uuid::Uuid;

#[derive(Debug, Clone, sqlx::Type, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[sqlx(type_name = "withdrawal_status", rename_all = "lowercase")]
#[serde(rename_all = "lowercase")]
pub enum WithdrawalStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, sqlx::FromRow, serde::Serialize)]
pub struct WithdrawalRow {
    pub id:          Uuid,
    pub user_id:     Uuid,
    pub amount:      i64,
    pub status:      WithdrawalStatus,
    pub reference:   Option<String>,
    pub reviewed_by: Option<Uuid>,
    pub reviewed_at: Option<DateTime<Utc>>,
    pub created_at:  DateTime<Utc>,
}

impl Db {
    pub async fn create_withdrawal(&self, user_id: UserId, amount: i64) -> AppResult<WithdrawalRow> {
        let mut tx = self.pool.begin().await?;

        let affected = sqlx::query(
            "UPDATE balances
             SET available = available - $2,
                 reserved  = reserved  + $2,
                 version   = version   + 1
             WHERE user_id = $1 AND available >= $2",
        )
        .bind(user_id.0)
        .bind(amount)
        .execute(&mut *tx)
        .await?
        .rows_affected();

        if affected == 0 {
            tx.rollback().await?;
            let bal = self.get_balance(user_id).await?;
            return Err(AppError::InsufficientBalance {
                needed:    amount as u64,
                available: bal.available as u64,
            });
        }

        let row = sqlx::query_as::<_, WithdrawalRow>(
            "INSERT INTO withdrawals (user_id, amount) VALUES ($1, $2) RETURNING *",
        )
        .bind(user_id.0)
        .bind(amount)
        .fetch_one(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO withdrawal_holds (withdrawal_id, user_id, amount) VALUES ($1, $2, $3)",
        )
        .bind(row.id)
        .bind(user_id.0)
        .bind(amount)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        Ok(row)
    }

    pub async fn approve_withdrawal(&self, id: Uuid, admin_id: UserId) -> AppResult<WithdrawalRow> {
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query_as::<_, WithdrawalRow>(
            "UPDATE withdrawals
             SET status      = 'approved',
                 reviewed_by = $2,
                 reviewed_at = now()
             WHERE id = $1 AND status = 'pending'
             RETURNING *",
        )
        .bind(id)
        .bind(admin_id.0)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("pending withdrawal {id}")))?;

        sqlx::query(
            "UPDATE balances
             SET reserved = GREATEST(0, reserved - $2),
                 version  = version + 1
             WHERE user_id = $1",
        )
        .bind(row.user_id)
        .bind(row.amount)
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM withdrawal_holds WHERE withdrawal_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(row)
    }

    pub async fn reject_withdrawal(&self, id: Uuid, admin_id: UserId) -> AppResult<WithdrawalRow> {
        let mut tx = self.pool.begin().await?;

        let row = sqlx::query_as::<_, WithdrawalRow>(
            "UPDATE withdrawals
             SET status      = 'rejected',
                 reviewed_by = $2,
                 reviewed_at = now()
             WHERE id = $1 AND status = 'pending'
             RETURNING *",
        )
        .bind(id)
        .bind(admin_id.0)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| AppError::NotFound(format!("pending withdrawal {id}")))?;

        sqlx::query(
            "UPDATE balances
             SET available = available + $2,
                 reserved  = GREATEST(0, reserved - $2),
                 version   = version   + 1
             WHERE user_id = $1",
        )
        .bind(row.user_id)
        .bind(row.amount)
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM withdrawal_holds WHERE withdrawal_id = $1")
            .bind(id)
            .execute(&mut *tx)
            .await?;

        tx.commit().await?;
        Ok(row)
    }

    pub async fn list_withdrawals(
        &self,
        user_id: Option<UserId>,
        status:  Option<WithdrawalStatus>,
        limit:   i64,
    ) -> AppResult<Vec<WithdrawalRow>> {
        match (user_id, status) {
            (Some(uid), Some(s)) => {
                sqlx::query_as::<_, WithdrawalRow>(
                    "SELECT * FROM withdrawals WHERE user_id = $1 AND status = $2
                     ORDER BY created_at DESC LIMIT $3",
                )
                .bind(uid.0).bind(s).bind(limit)
                .fetch_all(&self.pool).await.map_err(Into::into)
            }
            (Some(uid), None) => {
                sqlx::query_as::<_, WithdrawalRow>(
                    "SELECT * FROM withdrawals WHERE user_id = $1
                     ORDER BY created_at DESC LIMIT $2",
                )
                .bind(uid.0).bind(limit)
                .fetch_all(&self.pool).await.map_err(Into::into)
            }
            (None, Some(s)) => {
                sqlx::query_as::<_, WithdrawalRow>(
                    "SELECT * FROM withdrawals WHERE status = $1
                     ORDER BY created_at DESC LIMIT $2",
                )
                .bind(s).bind(limit)
                .fetch_all(&self.pool).await.map_err(Into::into)
            }
            (None, None) => {
                sqlx::query_as::<_, WithdrawalRow>(
                    "SELECT * FROM withdrawals ORDER BY created_at DESC LIMIT $1",
                )
                .bind(limit)
                .fetch_all(&self.pool).await.map_err(Into::into)
            }
        }
    }

    pub async fn expire_idempotency_keys(&self) -> AppResult<u64> {
        let n = sqlx::query("DELETE FROM idempotency_keys WHERE expires_at < now()")
            .execute(&self.pool)
            .await?
            .rows_affected();
        Ok(n)
    }
}
