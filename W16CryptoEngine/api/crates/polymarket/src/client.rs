use crate::types::{Market, MarketDataEvent, Balance, UserProfile, OrderStatus, OrderBook, Trade, CreateOrderRequest, CreateOrderResponse, Order, Side};
use common::{AppError, AppResult, CircuitBreaker, CircuitBreakerConfig};
use reqwest::{Client, RequestBuilder, Response};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};
const POLYMARKET_HOST: &str = "clob.polymarket.com";
pub struct PolymarketClient {
    client: Client,
    base_url: String,
    api_key: String,
    api_secret: String,
    api_passphrase: String,
    circuit_breaker: Option<Arc<CircuitBreaker>>,
}

impl PolymarketClient {
    pub fn new(api_key: String, api_secret: String, api_passphrase: String) -> Result<Self, AppError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(30))
            .pool_max_idle_per_host(10)
            .pool_idle_timeout(Duration::from_secs(60))
            .build()
            .map_err(|e| AppError::Internal(format!("Failed to create HTTP client: {}", e)))?;

        Ok(Self {
            client,
            base_url: POLYMARKET_API_URL.to_string(),
            api_key,
            api_secret,
            api_passphrase,
            circuit_breaker: None,
        })
    }

    pub fn with_circuit_breaker(mut self, config: CircuitBreakerConfig) -> Self {
        self.circuit_breaker = Some(Arc::new(CircuitBreaker::new("polymarket_api", config)));
        self
    }

    async fn check_circuit_breaker(&self) -> AppResult<()> {
        if let Some(cb) = &self.circuit_breaker {
            if !cb.allow_call().await {
                return Err(AppError::External("Circuit breaker open".to_string()));
            }
        }
        Ok(())
    }

    async fn record_success(&self) {
        if let Some(cb) = &self.circuit_breaker {
            cb.record_success().await;
        }
    }

    async fn record_failure(&self) {
        if let Some(cb) = &self.circuit_breaker {
            cb.record_failure().await;
        }
    }

    fn auth_headers(&self, builder: RequestBuilder) -> RequestBuilder {
        let timestamp = chrono::Utc::now().timestamp();
        let message = format!("{}GET/clob/config", timestamp);
        let signature = self.sign_message(&message);

        builder
            .header("POLYMARKET-API-KEY", &self.api_key)
            .header("POLYMARKET-SIGNATURE", signature)
            .header("POLYMARKET-TIMESTAMP", timestamp.to_string())
            .header("POLYMARKET-PASSPHRASE", &self.api_passphrase)
            .header("Host", POLYMARKET_HOST)
    }

    fn sign_message(&self, message: &str) -> String {
        use sha3::{Digest, Keccak256};
        let mut hasher = Keccak256::new();
        hasher.update(message.as_bytes());
        let result = hasher.finalize();
        hex::encode(result)
    }

    pub async fn get_markets(&self) -> Result<Vec<Market>, AppError> {
        let url = format!("{}/markets", self.base_url);
        let request = self
            .client
            .get(&url)
            .query(&[("active", "true"), ("archived", "false")]);

        let response = self.auth_headers(request).send().await.map_err(|e| {
            error!(error = %e, "Markets fetch failed");
            AppError::Internal(format!("Polymarket API error: {}", e))
        })?;

        self.handle_response(response).await
    }

    pub async fn get_market(&self, market_id: &str) -> Result<Market, AppError> {
        let url = format!("{}/markets/{}", self.base_url, market_id);
        let request = self.client.get(&url);

        let response = self.auth_headers(request).send().await.map_err(|e| {
            error!(error = %e, market_id, "Market fetch failed");
            AppError::Internal(format!("Polymarket API error: {}", e))
        })?;

        self.handle_response(response).await
    }

    pub async fn get_orderbook(
        &self,
        market_id: &str,
        asset_id: &str,
    ) -> Result<OrderBook, AppError> {
        let url = format!("{}/markets/{}/orderbook", self.base_url, market_id);
        let request = self
            .client
            .get(&url)
            .query(&[("asset_id", asset_id)]);

        let response = self.auth_headers(request).send().await.map_err(|e| {
            error!(error = %e, market_id, "Orderbook fetch failed");
            AppError::Internal(format!("Polymarket API error: {}", e))
        })?;

        self.handle_response(response).await
    }

    pub async fn get_trades(
        &self,
        market_id: &str,
        asset_id: &str,
        limit: usize,
    ) -> Result<Vec<Trade>, AppError> {
        let url = format!("{}/markets/{}/trades", self.base_url, market_id);
        let limit_str = limit.to_string();
        let request = self
            .client
            .get(&url)
            .query(&[("asset_id", asset_id), ("limit", limit_str.as_str())]);

        let response = self.auth_headers(request).send().await.map_err(|e| {
            error!(error = %e, market_id, "Trades fetch failed");
            AppError::Internal(format!("Polymarket API error: {}", e))
        })?;

        self.handle_response(response).await
    }

    pub async fn create_order(
        &self,
        order: &CreateOrderRequest,
    ) -> Result<CreateOrderResponse, AppError> {
        self.check_circuit_breaker().await?;

        let url = format!("{}/orders", self.base_url);

        let request_body = serde_json::json!({
            "marketId": order.market_id,
            "assetId": order.asset_id,
            "price": order.price,
            "size": order.size,
            "side": match order.side {
                Side::Buy => "buy",
                Side::Sell => "sell",
            },
            "nonce": order.nonce,
            "takerFee": order.taker_fee,
            "makerFee": order.maker_fee,
            "selfTradeBehavior": order.self_trade_behavior,
            "timeInForce": order.time_in_force,
            "expiration": order.expiration,
            "negRisk": order.neg_risk,
        });

        let request = self
            .client
            .post(&url)
            .json(&request_body);

        let result = async {
            let response = self.auth_headers(request).send().await.map_err(|e| {
                error!(error = %e, "Order create failed");
                AppError::Internal(format!("Polymarket API error: {}", e))
            })?;
            self.handle_response(response).await
        }.await;

        match &result {
            Ok(_) => self.record_success().await,
            Err(_) => self.record_failure().await,
        }

        result
    }

    pub async fn cancel_order(&self, order_id: &str) -> Result<(), AppError> {
        let url = format!("{}/orders/{}", self.base_url, order_id);
        let request = self.client.delete(&url);

        let response = self.auth_headers(request).send().await.map_err(|e| {
            error!(error = %e, order_id, "Order cancel failed");
            AppError::External(format!("Polymarket API error: {}", e))
        })?;

        if response.status().is_success() {
            Ok(())
        } else {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            Err(AppError::Internal(format!(
                "Failed to cancel order: {} - {}",
                status, text
            )))
        }
    }

    pub async fn get_orders(
        &self,
        market_id: Option<&str>,
        status: Option<OrderStatus>,
    ) -> Result<Vec<Order>, AppError> {
        let url = format!("{}/orders", self.base_url);
        let mut request = self.client.get(&url);

        if let Some(market_id) = market_id {
            request = request.query(&[("marketId", market_id)]);
        }

        if let Some(status) = status {
            let status_str = match status {
                OrderStatus::Open => "open",
                OrderStatus::Filled => "filled",
                OrderStatus::Cancelled => "cancelled",
                OrderStatus::Live => "live",
                OrderStatus::PartiallyFilled => "partially_filled",
            };
            request = request.query(&[("status", status_str)]);
        }

        let response = self.auth_headers(request).send().await.map_err(|e| {
            error!(error = %e, "Orders fetch failed");
            AppError::Internal(format!("Polymarket API error: {}", e))
        })?;

        self.handle_response(response).await
    }

    pub async fn get_balance(&self) -> Result<Vec<Balance>, AppError> {
        let url = format!("{}/balance", self.base_url);
        let request = self.client.get(&url);

        let response = self.auth_headers(request).send().await.map_err(|e| {
            error!(error = %e, "Balance fetch failed");
            AppError::Internal(format!("Polymarket API error: {}", e))
        })?;

        self.handle_response(response).await
    }

    pub async fn get_user_profile(&self) -> Result<UserProfile, AppError> {
        let url = format!("{}/profile", self.base_url);
        let request = self.client.get(&url);

        let response = self.auth_headers(request).send().await.map_err(|e| {
            error!(error = %e, "Profile fetch failed");
            AppError::Internal(format!("Polymarket API error: {}", e))
        })?;

        self.handle_response(response).await
    }

    pub async fn get_api_key_credentials(&self) -> Result<serde_json::Value, AppError> {
        let url = format!("{}/auth/api-key-nonce", self.base_url);
        let request = self.client.get(&url);

        let response = self.auth_headers(request).send().await.map_err(|e| {
            error!(error = %e, "API credentials fetch failed");
            AppError::Internal(format!("Polymarket API error: {}", e))
        })?;

        self.handle_response(response).await
    }

    async fn handle_response<T: serde::de::DeserializeOwned>(
        &self,
        response: Response,
    ) -> Result<T, AppError> {
        let status = response.status();

        if status.is_success() {
            response.json().await.map_err(|e| {
                error!(error = %e, "Response parse failed");
                AppError::Internal(format!("JSON parse error: {}", e))
            })
        } else {
            let text = response.text().await.unwrap_or_default();
            error!(status = %status, body = %text, "API error response");
            Err(AppError::Internal(format!(
                "Polymarket API returned {}: {}",
                status, text
            )))
        }
    }
}
