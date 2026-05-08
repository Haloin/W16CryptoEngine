use crate::Db;
use common::{AppResult, UserId};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdempotencyStatus {
    Fresh,
    Processing,
    Complete,
}

impl Db {
    pub async fn claim_idempotency_key(
        &self,
        user_id: UserId,
        key:     &str,
    ) -> AppResult<IdempotencyStatus> {
        let result = sqlx::query(
            "INSERT INTO idempotency_keys (user_id, key, status)
             VALUES ($1, $2, 1)
             ON CONFLICT (user_id, key) DO NOTHING",
        )
        .bind(user_id.0)
        .bind(key)
        .execute(&self.pool)
        .await?;

        if result.rows_affected() == 0 {
            let status: i16 = sqlx::query_scalar(
                "SELECT status FROM idempotency_keys WHERE user_id = $1 AND key = $2",
            )
            .bind(user_id.0)
            .bind(key)
            .fetch_one(&self.pool)
            .await?;

            return Ok(if status == 2 {
                IdempotencyStatus::Complete
            } else {
                IdempotencyStatus::Processing
            });
        }

        Ok(IdempotencyStatus::Fresh)
    }

    pub async fn complete_idempotency_key(
        &self,
        user_id:  UserId,
        key:      &str,
        response: Value,
    ) -> AppResult<()> {
        sqlx::query(
            "UPDATE idempotency_keys
             SET status = 2, response = $3
             WHERE user_id = $1 AND key = $2",
        )
        .bind(user_id.0)
        .bind(key)
        .bind(response)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn get_idempotency_response(
        &self,
        user_id: UserId,
        key:     &str,
    ) -> AppResult<Option<Value>> {
        let response: Option<Value> = sqlx::query_scalar(
            "SELECT response FROM idempotency_keys WHERE user_id = $1 AND key = $2 AND status = 2",
        )
        .bind(user_id.0)
        .bind(key)
        .fetch_optional(&self.pool)
        .await?
        .flatten();
        Ok(response)
    }
}
