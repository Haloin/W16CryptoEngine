use crate::Db;
use chrono::{DateTime, Utc};
use common::{AppError, AppResult, MarketId, MarketStatus};
use common::types::Market;
use common::types::Outcome;
use uuid::Uuid;

#[derive(sqlx::FromRow)]
pub struct MarketRow {
    pub id:          Uuid,
    pub title:       String,
    pub description: String,
    pub status:      MarketStatus,
    pub created_at:  DateTime<Utc>,
    pub resolves_at: Option<DateTime<Utc>>,
    pub settled_at:  Option<DateTime<Utc>>,
    pub outcome:     Option<Outcome>,
}

impl From<MarketRow> for Market {
    fn from(r: MarketRow) -> Self {
        Self {
            id:          MarketId(r.id),
            title:       r.title,
            description: r.description,
            status:      r.status,
            created_at:  r.created_at,
            resolves_at: r.resolves_at,
            settled_at:  r.settled_at,
            outcome:     r.outcome,
        }
    }
}

impl Db {
    pub async fn insert_market(
        &self,
        title:       &str,
        description: &str,
        resolves_at: Option<DateTime<Utc>>,
    ) -> AppResult<Market> {
        sqlx::query_as::<_, MarketRow>(
            "INSERT INTO markets (title, description, resolves_at)
             VALUES ($1, $2, $3)
             RETURNING *",
        )
        .bind(title)
        .bind(description)
        .bind(resolves_at)
        .fetch_one(&self.pool)
        .await
        .map(Into::into)
        .map_err(Into::into)
    }

    pub async fn get_market(&self, id: MarketId) -> AppResult<Market> {
        sqlx::query_as::<_, MarketRow>("SELECT * FROM markets WHERE id = $1")
            .bind(id.0)
            .fetch_optional(&self.pool)
            .await?
            .ok_or_else(|| AppError::MarketNotFound(id.to_string()))
            .map(Into::into)
    }

    pub async fn list_markets(&self, status: Option<MarketStatus>) -> AppResult<Vec<Market>> {
        let rows = match status {
            Some(s) => {
                sqlx::query_as::<_, MarketRow>(
                    "SELECT * FROM markets WHERE status = $1 ORDER BY created_at DESC LIMIT 200",
                )
                .bind(s)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query_as::<_, MarketRow>(
                    "SELECT * FROM markets ORDER BY created_at DESC LIMIT 200",
                )
                .fetch_all(&self.pool)
                .await?
            }
        };
        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn set_market_status(&self, id: MarketId, status: MarketStatus) -> AppResult<()> {
        let affected = sqlx::query("UPDATE markets SET status = $2 WHERE id = $1")
            .bind(id.0)
            .bind(status)
            .execute(&self.pool)
            .await?
            .rows_affected();

        if affected == 0 {
            return Err(AppError::MarketNotFound(id.to_string()));
        }
        Ok(())
    }

    pub async fn next_sequence(&self, market_id: MarketId) -> AppResult<u64> {
        let seq: i64 = sqlx::query_scalar(
            "INSERT INTO engine_sequences (market_id, last_seq) VALUES ($1, 1)
             ON CONFLICT (market_id) DO UPDATE
             SET last_seq = engine_sequences.last_seq + 1
             RETURNING last_seq",
        )
        .bind(market_id.0)
        .fetch_one(&self.pool)
        .await?;

        Ok(seq as u64)
    }
}
