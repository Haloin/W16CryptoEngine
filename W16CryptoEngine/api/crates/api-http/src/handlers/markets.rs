use axum::{
    extract::{Path, Query, State},
    Json,
};
use chrono::{DateTime, Utc};
use common::{AppError, AppResult, Market, MarketId, MarketStatus};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    middleware::auth::get_auth_context,
    state::AppState,
};

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateMarketRequest {
    pub title:       String,
    pub description: String,
    pub resolves_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct ListMarketsQuery {
    pub status: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct SettleRequest {
    pub outcome: String,
}

#[utoipa::path(get, path = "/markets", responses((status = 200, body = Vec<Market>)))]
pub async fn list_markets(
    State(state): State<AppState>,
    Query(params): Query<ListMarketsQuery>,
) -> AppResult<Json<Vec<Market>>> {
    let status = params.status.as_deref().and_then(|s| match s {
        "open"      => Some(MarketStatus::Open),
        "paused"    => Some(MarketStatus::Paused),
        "settled"   => Some(MarketStatus::Settled),
        "cancelled" => Some(MarketStatus::Cancelled),
        _           => None,
    });

    let markets = state.markets.list(status).await?;
    Ok(Json(markets))
}

#[utoipa::path(get, path = "/markets/{id}", responses((status = 200, body = Market)))]
pub async fn get_market(
    State(state): State<AppState>,
    Path(id):     Path<Uuid>,
) -> AppResult<Json<Market>> {
    let market = state.markets.get(MarketId(id)).await?;
    Ok(Json(market))
}

#[utoipa::path(post, path = "/markets", request_body = CreateMarketRequest, responses((status = 201, body = Market)))]
pub async fn create_market(
    State(state): State<AppState>,
    Extension(auth_context): Extension<AuthContext>,
    Json(payload):    Json<CreateMarketRequest>,
) -> AppResult<(axum::http::StatusCode, Json<Market>)> {
    if !auth_context.is_admin {
        return Err(AppError::Forbidden);
    }
    let user_id = common::UserId(uuid::Uuid::parse_str(&auth_context.user_id).map_err(|_| AppError::Unauthorized)?);
    let market  = state
        .markets
        .create(payload.title, payload.description, payload.resolves_at, user_id)
        .await?;

    Ok((axum::http::StatusCode::CREATED, Json(market)))
}

#[utoipa::path(post, path = "/markets/{id}/pause", responses((status = 204)))]
pub async fn pause_market(
    State(state): State<AppState>,
    Extension(auth_context): Extension<AuthContext>,
    Path(id):     Path<Uuid>,
) -> AppResult<axum::http::StatusCode> {
    if !auth_context.is_admin {
        return Err(AppError::Forbidden);
    }
    let user_id = common::UserId(uuid::Uuid::parse_str(&auth_context.user_id).map_err(|_| AppError::Unauthorized)?);
    state.markets.pause(MarketId(id), user_id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/markets/{id}/reopen", responses((status = 204)))]
pub async fn reopen_market(
    State(state): State<AppState>,
    Extension(auth_context): Extension<AuthContext>,
    Path(id):     Path<Uuid>,
) -> AppResult<axum::http::StatusCode> {
    if !auth_context.is_admin {
        return Err(AppError::Forbidden);
    }
    let user_id = common::UserId(uuid::Uuid::parse_str(&auth_context.user_id).map_err(|_| AppError::Unauthorized)?);
    state.markets.reopen(MarketId(id), user_id).await?;
    Ok(axum::http::StatusCode::NO_CONTENT)
}

#[utoipa::path(post, path = "/markets/{id}/settle", request_body = SettleRequest, responses((status = 200)))]
pub async fn settle_market(
    State(state): State<AppState>,
    Extension(auth_context): Extension<AuthContext>,
    Path(id):     Path<Uuid>,
    Json(payload): Json<SettleRequest>,
) -> AppResult<Json<serde_json::Value>> {
    if !auth_context.is_admin {
        return Err(AppError::Forbidden);
    }
    use common::Outcome;

    let outcome = match payload.outcome.to_lowercase().as_str() {
        "yes" => Outcome::Yes,
        "no"  => Outcome::No,
        _     => return Err(common::AppError::Validation("outcome must be 'yes' or 'no'".into())),
    };

    let user_id = common::UserId(uuid::Uuid::parse_str(&auth_context.user_id).map_err(|_| AppError::Unauthorized)?);
    let summary = state.settlement.settle(MarketId(id), outcome, user_id).await?;

    Ok(Json(serde_json::json!({
        "market_id":    summary.market_id.to_string(),
        "outcome":      format!("{:?}", summary.outcome),
        "payout_count": summary.payouts.len(),
        "total_payout": summary.total_payout,
    })))
}
