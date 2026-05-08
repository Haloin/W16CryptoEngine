use common::{MarketId, MarketStatus};
use common::types::Market;
use moka::future::Cache;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct MarketCache {
    markets: Cache<MarketId, Arc<Market>>,
    status:  Cache<MarketId, MarketStatus>,
}

impl MarketCache {
    pub fn new(ttl: Duration, capacity: u64) -> Self {
        Self {
            markets: Cache::builder()
                .time_to_live(ttl)
                .max_capacity(capacity)
                .build(),
            status: Cache::builder()
                .time_to_live(Duration::from_secs(5))
                .max_capacity(capacity)
                .build(),
        }
    }

    pub async fn get_market(&self, id: &MarketId) -> Option<Arc<Market>> {
        self.markets.get(id).await
    }

    pub async fn set_market(&self, market: Market) {
        let id = market.id;
        self.markets.insert(id, Arc::new(market)).await;
    }

    pub async fn invalidate(&self, id: &MarketId) {
        self.markets.invalidate(id).await;
        self.status.invalidate(id).await;
    }

    pub async fn get_status(&self, id: &MarketId) -> Option<MarketStatus> {
        self.status.get(id).await
    }

    pub async fn set_status(&self, id: MarketId, status: MarketStatus) {
        self.status.insert(id, status).await;
    }
}
