use std::sync::Arc;
use common::{AppError, OrderRequest, Side};
use analytics::risk::RiskManager;
use crate::reconciliation::{OrderReconciler, PendingOrder};
use polymarket::client::PolymarketClient;
use polymarket::types::{CreateOrderRequest, Side as PolymarketSide};

pub mod reconciliation;

pub struct OrderService {
    risk_manager: Arc<RiskManager>,
    reconciler: Arc<OrderReconciler>,
    exchange_client: Arc<PolymarketClient>,
}

impl OrderService {
    pub fn new(
        risk_manager: Arc<RiskManager>,
        reconciler: Arc<OrderReconciler>,
        exchange_client: Arc<PolymarketClient>,
    ) -> Self {
        Self {
            risk_manager,
            reconciler,
            exchange_client,
        }
    }

    pub async fn place_order(&self, user_id: common::UserId, req: OrderRequest) -> Result<(), AppError> {
        let order = common::types::Order {
            id: common::types::OrderId::new(),
            user_id,
            market_id: req.market_id,
            side: req.side,
            kind: req.kind,
            price: req.price,
            quantity: req.quantity,
            filled_quantity: common::types::Quantity(0),
            status: common::types::OrderStatus::Open,
            sequence: 0,
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        };

        let current_exposure = self.risk_manager.get_user_exposure(user_id).await;
        let risk_result = self.risk_manager.check_order_risk(user_id, &order, current_exposure).await;
        
        if !risk_result.allowed {
            return Err(AppError::RiskLimitExceeded(risk_result.reason.unwrap_or_default()));
        }

        let price = req.price.ok_or_else(|| AppError::BadRequest("Price required for limit order".to_string()))?;
        
        let create_req = CreateOrderRequest {
            market_id: req.market_id.to_string(),
            asset_id: String::new(),
            price: price.to_float(),
            size: req.quantity.to_float(),
            side: if req.side == Side::Buy { PolymarketSide::Buy } else { PolymarketSide::Sell },
            taker_fee: None,
            maker_fee: None,
            nonce: chrono::Utc::now().timestamp_micros() as u64,
            self_trade_behavior: None,
            time_in_force: "GTC".to_string(),
            expiration: None,
            neg_risk: None,
        };

        let res = self.exchange_client.create_order(&create_req).await?;
        
        self.reconciler.register(PendingOrder {
            order_id: res.order_id,
            market_id: req.market_id.to_string(),
            quantity: req.quantity.to_float(),
            timestamp: chrono::Utc::now().timestamp(),
        }).await;

        Ok(())
    }

    pub async fn submit(&self, req: OrderRequest) -> Result<common::types::Order, AppError> {
        let user_id = common::types::UserId::new();
        
        self.place_order(user_id, req.clone()).await.map(|_| {
            common::types::Order {
                id: common::types::OrderId::new(),
                user_id,
                market_id: req.market_id,
                side: req.side,
                kind: req.kind,
                price: req.price,
                quantity: req.quantity,
                filled_quantity: common::types::Quantity(0),
                status: common::types::OrderStatus::Open,
                sequence: 0,
                created_at: chrono::Utc::now(),
                updated_at: chrono::Utc::now(),
            }
        })
    }

    pub async fn cancel(&self, user_id: common::types::UserId, order_id: common::types::OrderId) -> Result<common::types::Order, AppError> {
        Err(AppError::BadRequest("Order cancellation not implemented".to_string()))
    }

    pub async fn get(&self, order_id: common::types::OrderId) -> Result<common::types::Order, AppError> {
        Err(AppError::NotFound("Order not found".to_string()))
    }

    pub async fn list_for_user(&self, user_id: common::types::UserId, market_id: Option<common::types::MarketId>, limit: Option<u32>) -> Result<Vec<common::types::Order>, AppError> {
        Ok(vec![])
    }
}