use axum::{extract::{State, Extension}, http::HeaderMap, Json};
use common::{AppError, AppResult};
use db::idempotency::IdempotencyStatus;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::{
    middleware::auth::AuthContext,
    state::AppState,
};

#[derive(Debug, Deserialize, ToSchema)]
pub struct DepositRequest {
    pub user_id:   Uuid,
    pub amount:    f64,
    pub reference: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct DepositResponse {
    pub user_id:   Uuid,
    pub amount:    i64,
    pub reference: String,
}

#[utoipa::path(post, path = "/admin/deposits", request_body = DepositRequest, responses((status = 201, body = DepositResponse)))]
pub async fn deposit(
    State(state): State<AppState>,
    Extension(auth_context): Extension<AuthContext>,
    headers:      HeaderMap,
    Json(payload): Json<DepositRequest>,
) -> AppResult<(axum::http::StatusCode, Json<DepositResponse>)> {
    if !auth_context.is_admin {
        return Err(AppError::Forbidden);
    }
    
    if payload.amount <= 0.0 {
        return Err(AppError::Validation("amount must be positive".into()));
    }
    if payload.reference.trim().is_empty() {
        return Err(AppError::Validation("reference is required".into()));
    }

    let admin_id    = common::UserId(uuid::Uuid::parse_str(&auth_context.user_id).map_err(|_| AppError::Unauthorized)?);
    let user_id     = common::UserId(payload.user_id);
    let amount_units = (payload.amount * 10_000.0).round() as i64;

    if amount_units <= 0 {
        return Err(AppError::Validation("amount too small".into()));
    }

    let idem_key = headers
        .get("idempotency-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| Uuid::new_v4().to_string());

    match state.db.claim_idempotency_key(admin_id, &idem_key).await? {
        IdempotencyStatus::Complete => {
            if let Some(cached) = state.db.get_idempotency_response(admin_id, &idem_key).await? {
                let resp: DepositResponse = serde_json::from_value(cached)
                    .map_err(|e| AppError::Internal(e.to_string()))?;
                return Ok((axum::http::StatusCode::OK, Json(resp)));
            }
        }
        IdempotencyStatus::Processing => {
            return Err(AppError::Conflict("request already in progress".into()));
        }
        IdempotencyStatus::Fresh => {}
    }

    state
        .db
        .record_deposit(user_id, amount_units, &payload.reference)
        .await?;

    state
        .db
        .audit(
            Some(admin_id),
            "deposit",
            "user",
            Some(&user_id.to_string()),
            Some(serde_json::json!({
                "amount": amount_units,
                "reference": payload.reference
            })),
        )
        .await?;

    let resp = DepositResponse {
        user_id:   payload.user_id,
        amount:    amount_units,
        reference: payload.reference,
    };

    state
        .db
        .complete_idempotency_key(
            admin_id,
            &idem_key,
            serde_json::to_value(&resp).unwrap_or_default(),
        )
        .await?;

    Ok((axum::http::StatusCode::CREATED, Json(resp)))
}
