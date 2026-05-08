use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("Internal error: {0}")]
    Internal(String),
    #[error("External exchange error: {0}")]
    External(String),
    #[error("Validation failed: {0}")]
    Validation(String),
    #[error("Risk limit exceeded: {0}")]
    RiskLimitExceeded(String),
    #[error("Slippage limit exceeded")]
    SlippageExceeded,
    #[error("Circuit breaker is open")]
    CircuitBreakerOpen,
    #[error("DB error: {0}")]
    Database(String),
    #[error("Chain error: {0}")]
    Blockchain(String),
    #[error("NATS error: {0}")]
    Messaging(String),
    #[error("Insufficient balance: needed {needed}, available {available}")]
    InsufficientBalance {
        needed: u64,
        available: u64,
    },
    #[error("Market not found: {0}")]
    MarketNotFound(String),
    #[error("Order not found: {0}")]
    OrderNotFound(String),
    #[error("Market closed")]
    MarketNotOpen,
    #[error("Resource missing: {0}")]
    NotFound(String),
    #[error("Unauthorized")]
    Unauthorized,
    #[error("Access forbidden")]
    Forbidden,
    #[error("Conflict: {0}")]
    Conflict(String),
    #[error("Invalid request: {0}")]
    BadRequest(String),
}

impl From<sqlx::Error> for AppError {
    fn from(e: sqlx::Error) -> Self {
        AppError::Database(e.to_string())
    }
}

impl From<uuid::Error> for AppError {
    fn from(e: uuid::Error) -> Self {
        AppError::BadRequest(format!("Invalid UUID: {}", e))
    }
}

impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let (status, error_message) = match self {
            AppError::Internal(msg) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, msg),
            AppError::External(msg) => (axum::http::StatusCode::BAD_GATEWAY, msg),
            AppError::Validation(msg) => (axum::http::StatusCode::BAD_REQUEST, msg),
            AppError::RiskLimitExceeded(msg) => (axum::http::StatusCode::FORBIDDEN, msg),
            AppError::SlippageExceeded => (axum::http::StatusCode::BAD_REQUEST, "Slippage limit exceeded".to_string()),
            AppError::CircuitBreakerOpen => (axum::http::StatusCode::SERVICE_UNAVAILABLE, "Circuit breaker is open".to_string()),
            AppError::Database(msg) => (axum::http::StatusCode::INTERNAL_SERVER_ERROR, msg),
            AppError::Blockchain(msg) => (axum::http::StatusCode::BAD_GATEWAY, msg),
            AppError::Messaging(msg) => (axum::http::StatusCode::SERVICE_UNAVAILABLE, msg),
            AppError::InsufficientBalance { needed, available } => (axum::http::StatusCode::BAD_REQUEST, format!("Insufficient balance: needed={}, available={}", needed, available)),
            AppError::MarketNotFound(msg) => (axum::http::StatusCode::NOT_FOUND, msg),
            AppError::OrderNotFound(msg) => (axum::http::StatusCode::NOT_FOUND, msg),
            AppError::MarketNotOpen => (axum::http::StatusCode::BAD_REQUEST, "Market is not open".to_string()),
            AppError::NotFound(msg) => (axum::http::StatusCode::NOT_FOUND, msg),
            AppError::Unauthorized => (axum::http::StatusCode::UNAUTHORIZED, "Unauthorized".to_string()),
            AppError::Forbidden => (axum::http::StatusCode::FORBIDDEN, "Forbidden".to_string()),
            AppError::Conflict(msg) => (axum::http::StatusCode::CONFLICT, msg),
            AppError::BadRequest(msg) => (axum::http::StatusCode::BAD_REQUEST, msg),
        };

        let body = axum::Json(serde_json::json!({
            "error": error_message
        }));
        (status, body).into_response()
    }
}

pub type AppResult<T> = Result<T, AppError>;