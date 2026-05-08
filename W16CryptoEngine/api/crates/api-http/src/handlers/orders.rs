use axum::{
    extract::{Path, Query, State, Extension},
    http::HeaderMap,
    Json,
};
use common::{AppError, AppResult, MarketId, Order, OrderId, OrderKind, Price, Quantity, Side};
use db::idempotency::IdempotencyStatus;
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    middleware::auth::AuthContext,
    state::AppState,
};

#[derive(Debug, Deserialize, ToSchema)]
pub struct PlaceOrderRequest {
    pub market_id: Uuid,
    pub side:     String,
    pub kind:     String,
    pub price:    Option<rust_decimal::Decimal>,
    pub quantity: rust_decimal::Decimal,
}

#[derive(Debug, Deserialize)]
pub struct ListOrdersQuery {
    pub market_id: Option<Uuid>,
    pub limit:     Option<i64>,
}

pub async fn place_order(
    State(state): State<AppState>,
    Extension(auth_context): Extension<AuthContext>,
    headers:      HeaderMap,
    Path(_id):    Path<Uuid>,
    Json(payload): Json<PlaceOrderRequest>,
) -> AppResult<(axum::http::StatusCode, Json<Order>)> {
    let user_id = common::UserId(uuid::Uuid::parse_str(&auth_context.user_id).map_err(|_| AppError::Unauthorized)?);

    let idem_key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    match state.db.claim_idempotency_key(user_id, &idem_key).await? {
        IdempotencyStatus::Complete => {
            if let Some(cached) = state.db.get_idempotency_response(user_id, &idem_key).await? {
                let order: Order = serde_json::from_value(cached)
                    .map_err(|e| common::AppError::Internal(e.to_string()))?;
                return Ok((axum::http::StatusCode::OK, Json(order)));
            }
        }
        IdempotencyStatus::Processing => {
            return Err(common::AppError::Conflict("duplicate request in flight".into()));
        }
        IdempotencyStatus::Fresh => {}
    }

   
    let side = match payload.side.to_lowercase().as_str() {
        "buy"  => Side::Buy,
        "sell" => Side::Sell,
        _      => return Err(common::AppError::Validation("side must be buy or sell".into())),
    };

    let kind = match payload.kind.to_lowercase().as_str() {
        "limit"  => OrderKind::Limit,
        "market" => OrderKind::Market,
        _        => return Err(common::AppError::Validation("kind must be limit or market".into())),
    };

    let price = payload
        .price
        .map(|p| {
            Price::from_decimal(p)
                .ok_or_else(|| common::AppError::Validation("price must be in (0, 1)".into()))
        })
        .transpose()?;

    if payload.quantity <= rust_decimal::Decimal::ZERO {
        return Err(common::AppError::Validation("quantity must be positive".into()));
    }

    let order = state
        .orders
        .submit(
            common::OrderRequest {
                market_id: MarketId(payload.market_id),
                side,
                kind,
                price,
                quantity: Quantity::from_decimal(payload.quantity),
            },
        )
        .await?;

    state
        .db
        .complete_idempotency_key(
            user_id,
            &idem_key,
            serde_json::to_value(&order).unwrap_or_default(),
        )
        .await?;

    Ok((axum::http::StatusCode::CREATED, Json(order)))
}

pub async fn cancel_order(
    State(state): State<AppState>,
    Extension(auth_context): Extension<AuthContext>,
    Path(id):     Path<Uuid>,
) -> AppResult<Json<Order>> {
    let user_id = common::UserId(uuid::Uuid::parse_str(&auth_context.user_id).map_err(|_| AppError::Unauthorized)?);
    let order   = state.orders.cancel(user_id, OrderId(id)).await?;
    Ok(Json(order))
}

pub async fn get_order(
    State(state): State<AppState>,
    Extension(auth_context): Extension<AuthContext>,
    Path(id):     Path<Uuid>,
) -> AppResult<Json<Order>> {
    let user_id = common::UserId(uuid::Uuid::parse_str(&auth_context.user_id).map_err(|_| AppError::Unauthorized)?);
    let order   = state.orders.get(OrderId(id)).await?;
    if order.user_id != user_id {
        return Err(common::AppError::Forbidden);
    }
    Ok(Json(order))
}

pub async fn list_orders(
    State(state): State<AppState>,
    Extension(auth_context): Extension<AuthContext>,
    Query(query): Query<ListOrdersQuery>,
) -> AppResult<Json<Vec<Order>>> {
    let user_id = common::UserId(uuid::Uuid::parse_str(&auth_context.user_id).map_err(|_| AppError::Unauthorized)?);
    let market_id = query.market_id.map(MarketId);
    let limit     = query.limit.unwrap_or(50).min(200);
    let orders   = state.orders.list_for_user(user_id, market_id, Some(limit as u32)).await?;
    Ok(Json(orders))
}
