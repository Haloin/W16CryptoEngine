use common::{AppError, AppResult};
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;

pub mod balances;
pub mod fills;
pub mod idempotency;
pub mod markets;
pub mod orders;
pub mod positions;
pub mod users;
pub mod withdrawals;

pub use balances::BalanceRow;
pub use fills::FillRow;
pub use idempotency::IdempotencyStatus;
pub use markets::MarketRow;
pub use orders::OrderRow;
pub use positions::PositionRow;
pub use users::UserRow;
pub use withdrawals::{WithdrawalRow, WithdrawalStatus};

#[derive(Clone)]
pub struct Db {
    pub(crate) pool: PgPool,
}

impl Db {
    pub async fn connect(
        url:             &str,
        max_connections: u32,
        min_connections: u32,
        connect_timeout: Duration,
        idle_timeout:    Duration,
        max_lifetime:    Duration,
    ) -> AppResult<Self> {
        let pool = PgPoolOptions::new()
            .max_connections(max_connections)
            .min_connections(min_connections)
            .acquire_timeout(connect_timeout)
            .idle_timeout(idle_timeout)
            .max_lifetime(max_lifetime)
            .connect(url)
            .await?;

        sqlx::migrate!("../../migrations")
            .run(&pool)
            .await
            .map_err(|e| AppError::Internal(format!("migration: {e}")))?;

        Ok(Self { pool })
    }

    pub async fn ping(&self) -> AppResult<()> {
        sqlx::query("SELECT 1")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
