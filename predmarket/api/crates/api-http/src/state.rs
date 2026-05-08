use analytics::AnalyticsEngine;
use auth::JwtService;
use cache::MarketCache;
use db::Db;
use markets::MarketService;
use messaging::Nats;
use orders::OrderService;
use settlement::SettlementService;
use std::sync::Arc;

#[derive(Clone)]
pub struct AppState {
    pub db:         Arc<Db>,
    pub nats:       Arc<Nats>,
    pub jwt:        Arc<JwtService>,
    pub orders:     Arc<OrderService>,
    pub markets:    Arc<MarketService>,
    pub settlement: Arc<SettlementService>,
    pub cache:      Arc<MarketCache>,
    pub analytics:  Arc<AnalyticsEngine>,
}
