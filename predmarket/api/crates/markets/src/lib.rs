use chrono::{DateTime, Utc};
use common::{AppError, AppResult, Market, MarketId, MarketStatus, UserId};
use db::Db;
use std::sync::Arc;
use tracing::instrument;

pub struct MarketService {
    db: Arc<Db>,
}

impl MarketService {
    pub fn new(db: Arc<Db>) -> Self {
        Self { db }
    }

    pub async fn create(
        &self,
        title:       String,
        description: String,
        resolves_at: Option<DateTime<Utc>>,
        created_by:  UserId,
    ) -> AppResult<Market> {
        let title = title.trim().to_string();
        if title.is_empty() {
            return Err(AppError::Validation("title is required".into()));
        }
        if title.len() > 200 {
            return Err(AppError::Validation("title exceeds 200 characters".into()));
        }

        let market = self.db.insert_market(&title, &description, resolves_at).await?;

        self.db
            .audit(Some(created_by), "create", "market", Some(&market.id.to_string()), None)
            .await?;

        Ok(market)
    }

    pub async fn get(&self, id: MarketId) -> AppResult<Market> {
        self.db.get_market(id).await
    }

    pub async fn list(&self, status: Option<MarketStatus>) -> AppResult<Vec<Market>> {
        self.db.list_markets(status).await
    }

    pub async fn pause(&self, id: MarketId, by: UserId) -> AppResult<()> {
        let market = self.db.get_market(id).await?;
        if market.status != MarketStatus::Open {
            return Err(AppError::BadRequest("only open markets can be paused".into()));
        }
        self.db.set_market_status(id, MarketStatus::Paused).await?;
        self.db.audit(Some(by), "pause", "market", Some(&id.to_string()), None).await?;
        Ok(())
    }

    pub async fn reopen(&self, id: MarketId, by: UserId) -> AppResult<()> {
        let market = self.db.get_market(id).await?;
        if market.status != MarketStatus::Paused {
            return Err(AppError::BadRequest("only paused markets can be reopened".into()));
        }
        self.db.set_market_status(id, MarketStatus::Open).await?;
        self.db.audit(Some(by), "reopen", "market", Some(&id.to_string()), None).await?;
        Ok(())
    }

    pub async fn cancel_market(&self, id: MarketId, by: UserId) -> AppResult<()> {
        let market = self.db.get_market(id).await?;
        if market.status == MarketStatus::Settled {
            return Err(AppError::BadRequest("cannot cancel a settled market".into()));
        }
        self.db.set_market_status(id, MarketStatus::Cancelled).await?;
        self.db.audit(Some(by), "cancel", "market", Some(&id.to_string()), None).await?;
        Ok(())
    }
}
