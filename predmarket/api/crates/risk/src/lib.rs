use common::{AppError, AppResult, MarketId, OrderRequest, Side, UserId};
use db::Db;
use std::sync::Arc;

pub struct RiskControls {
    db:           Arc<Db>,
    max_position: i64,
    min_price:    u32,
    max_price:    u32,
    max_quantity: u64,
}

impl RiskControls {
    pub fn new(db: Arc<Db>) -> Self {
        Self {
            db,
            max_position: 100_000_000,
            min_price:    1,
            max_price:    9999,
            max_quantity: 100_000_000,
        }
    }

    pub fn with_limits(
        db:           Arc<Db>,
        max_position: i64,
        min_price:    u32,
        max_price:    u32,
        max_quantity: u64,
    ) -> Self {
        Self {
            db,
            max_position,
            min_price,
            max_price,
            max_quantity,
        }
    }

    pub async fn check_order(&self, user_id: UserId, req: &OrderRequest) -> AppResult<()> {
        self.check_price_bands(req)?;
        self.check_quantity_limit(req)?;
        self.check_position_limit(user_id, req).await?;
        Ok(())
    }

    fn check_price_bands(&self, req: &OrderRequest) -> AppResult<()> {
        if let Some(price) = req.price {
            if price.0 < self.min_price || price.0 > self.max_price {
                return Err(AppError::Validation(format!(
                    "price {} outside allowed range [{}, {}]",
                    price.0, self.min_price, self.max_price
                )));
            }
        }
        Ok(())
    }

    fn check_quantity_limit(&self, req: &OrderRequest) -> AppResult<()> {
        if req.quantity.0 > self.max_quantity {
            return Err(AppError::Validation(format!(
                "quantity {} exceeds maximum allowed {}",
                req.quantity.0, self.max_quantity
            )));
        }
        Ok(())
    }

    async fn check_position_limit(&self, user_id: UserId, req: &OrderRequest) -> AppResult<()> {
        let net = self.db.get_position_for_user(user_id, req.market_id).await.unwrap_or(0);

        let delta = match req.side {
            Side::Buy  => req.quantity.0 as i64,
            Side::Sell => -(req.quantity.0 as i64),
        };

        let projected = net.saturating_add(delta);

        if projected.abs() > self.max_position {
            return Err(AppError::Validation(format!(
                "projected position {} would exceed limit {}",
                projected, self.max_position
            )));
        }

        Ok(())
    }
}
