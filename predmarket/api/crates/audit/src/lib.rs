use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditEvent {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub event_type: AuditEventType,
    pub user_id: Option<String>,
    pub session_id: Option<String>,
    pub ip_address: Option<String>,
    pub market_id: Option<String>,
    pub order_id: Option<String>,
    pub details: serde_json::Value,
    pub outcome: AuditOutcome,
    pub latency_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditEventType {
    OrderCreated,
    OrderCancelled,
    OrderFilled,
    DepositReceived,
    WithdrawalRequested,
    WithdrawalCompleted,
    Login,
    Logout,
    ApiKeyCreated,
    ApiKeyRevoked,
    MarketCreated,
    MarketResolved,
    RiskLimitBreached,
    CircuitBreakerTriggered,
    HftSignalGenerated,
    HftOrderSubmitted,
    ArbitrageOpportunityDetected,
    MlPredictionMade,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditOutcome {
    Success,
    Failure,
    Denied,
    Timeout,
    Error,
}

pub struct AuditLogger {
    sender: mpsc::UnboundedSender<AuditEvent>,
}

impl AuditLogger {
    pub fn new() -> (Self, AuditConsumer) {
        let (sender, receiver) = mpsc::unbounded_channel();
        let consumer = AuditConsumer::new(receiver);
        (Self { sender }, consumer)
    }

    pub fn log(&self, event: AuditEvent) {
        if let Err(e) = self.sender.send(event) {
            error!("Failed to send audit event: {}", e);
        }
    }

    pub fn log_order_created(
        &self,
        user_id: String,
        order_id: String,
        market_id: String,
        side: String,
        price: i64,
        quantity: i64,
        outcome: AuditOutcome,
    ) {
        self.log(AuditEvent {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type: AuditEventType::OrderCreated,
            user_id: Some(user_id),
            session_id: None,
            ip_address: None,
            market_id: Some(market_id),
            order_id: Some(order_id),
            details: serde_json::json!({
                "side": side,
                "price": price,
                "quantity": quantity,
            }),
            outcome,
            latency_ms: None,
        });
    }

    pub fn log_hft_signal(
        &self,
        market_id: String,
        signal_strength: f64,
        confidence: f64,
        latency_ns: u64,
    ) {
        self.log(AuditEvent {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type: AuditEventType::HftSignalGenerated,
            user_id: None,
            session_id: None,
            ip_address: None,
            market_id: Some(market_id),
            order_id: None,
            details: serde_json::json!({
                "signal_strength": signal_strength,
                "confidence": confidence,
            }),
            outcome: AuditOutcome::Success,
            latency_ms: Some(latency_ns / 1_000_000),
        });
    }

    pub fn log_risk_breach(&self, user_id: String, market_id: String, reason: String) {
        self.log(AuditEvent {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type: AuditEventType::RiskLimitBreached,
            user_id: Some(user_id),
            session_id: None,
            ip_address: None,
            market_id: Some(market_id),
            order_id: None,
            details: serde_json::json!({
                "reason": reason,
            }),
            outcome: AuditOutcome::Denied,
            latency_ms: None,
        });
    }

    pub fn log_circuit_breaker(&self, market_id: String, trigger: String) {
        self.log(AuditEvent {
            id: Uuid::new_v4(),
            timestamp: Utc::now(),
            event_type: AuditEventType::CircuitBreakerTriggered,
            user_id: None,
            session_id: None,
            ip_address: None,
            market_id: Some(market_id),
            order_id: None,
            details: serde_json::json!({
                "trigger": trigger,
            }),
            outcome: AuditOutcome::Denied,
            latency_ms: None,
        });
    }
}

pub struct AuditConsumer {
    receiver: mpsc::UnboundedReceiver<AuditEvent>,
}

impl AuditConsumer {
    fn new(receiver: mpsc::UnboundedReceiver<AuditEvent>) -> Self {
        Self { receiver }
    }

    pub async fn run(mut self) {
        while let Some(event) = self.receiver.recv().await {
            let json = match serde_json::to_string(&event) {
                Ok(j) => j,
                Err(e) => {
                    error!("Failed to serialize audit event: {}", e);
                    continue;
                }
            };

            info!(audit = true, event = %json, "Audit event");
        }
    }
}

#[derive(Clone)]
pub struct AuditLayer {
    logger: Arc<AuditLogger>,
}

impl AuditLayer {
    pub fn new(logger: Arc<AuditLogger>) -> Self {
        Self { logger }
    }
}

impl<S> tower::Layer<S> for AuditLayer {
    type Service = AuditService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        AuditService {
            inner,
            logger: self.logger.clone(),
        }
    }
}

#[derive(Clone)]
pub struct AuditService<S> {
    inner: S,
    logger: Arc<AuditLogger>,
}

impl<S, Req> tower::Service<Req> for AuditService<S>
where
    S: tower::Service<Req>,
    Req: std::fmt::Debug,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = S::Future;

    fn poll_ready(
        &mut self,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Req) -> Self::Future {
        self.inner.call(req)
    }
}
