use axum::{
    extract::{Path, Query, State, Extension},
    Json,
};
use common::{AppError, AppResult};
use db::withdrawals::{WithdrawalRow, WithdrawalStatus};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    middleware::auth::AuthContext,
    state::AppState,
};

#[derive(Debug, Deserialize)]
pub struct RequestWithdrawalBody {
    pub amount: f64,
}

#[derive(Debug, Deserialize)]
pub struct ListWithdrawalsQuery {
    pub status: Option<String>,
    pub limit:  Option<i64>,
}

fn parse_status(s: &str) -> Option<WithdrawalStatus> {
    match s {
        "pending"  => Some(WithdrawalStatus::Pending),
        "approved" => Some(WithdrawalStatus::Approved),
        "rejected" => Some(WithdrawalStatus::Rejected),
        _          => None,
    }
}

pub async fn request(
    State(state): State<AppState>,
    Extension(auth_context): Extension<AuthContext>,
    Json(body):   Json<RequestWithdrawalBody>,
) -> AppResult<(axum::http::StatusCode, Json<WithdrawalRow>)> {
    if body.amount <= 0.0 {
        return Err(AppError::Validation("amount must be positive".into()));
    }

    let user_id      = common::UserId(uuid::Uuid::parse_str(&auth_context.user_id).map_err(|_| AppError::Unauthorized)?);
    let amount_units = (body.amount * 10_000.0).round() as i64;

    if amount_units <= 0 {
        return Err(AppError::Validation("amount too small".into()));
    }

    let row = state.db.create_withdrawal(user_id, amount_units).await?;

    state
        .db
        .audit(
            Some(user_id),
            "request_withdrawal",
            "withdrawal",
            Some(&row.id.to_string()),
            Some(serde_json::json!({"amount": amount_units})),
        )
        .await?;

    Ok((axum::http::StatusCode::CREATED, Json(row)))
}

pub async fn approve(
    State(state): State<AppState>,
    Extension(auth_context): Extension<AuthContext>,
    Path(id):     Path<Uuid>,
) -> AppResult<Json<WithdrawalRow>> {
    if !auth_context.is_admin {
        return Err(AppError::Forbidden);
    }
    let admin_id = common::UserId(uuid::Uuid::parse_str(&auth_context.user_id).map_err(|_| AppError::Unauthorized)?);
    let row      = state.db.approve_withdrawal(id, admin_id).await?;

    state
        .db
        .audit(
            Some(admin_id),
            "approve_withdrawal",
            "withdrawal",
            Some(&id.to_string()),
            Some(serde_json::json!({"amount": row.amount})),
        )
        .await?;

    Ok(Json(row))
}

pub async fn reject(
    State(state): State<AppState>,
    Extension(auth_context): Extension<AuthContext>,
    Path(id):     Path<Uuid>,
) -> AppResult<Json<WithdrawalRow>> {
    if !auth_context.is_admin {
        return Err(AppError::Forbidden);
    }
    let admin_id = common::UserId(uuid::Uuid::parse_str(&auth_context.user_id).map_err(|_| AppError::Unauthorized)?);
    let row      = state.db.reject_withdrawal(id, admin_id).await?;

    state
        .db
        .audit(
            Some(admin_id),
            "reject_withdrawal",
            "withdrawal",
            Some(&id.to_string()),
            None,
        )
        .await?;

    Ok(Json(row))
}

pub async fn list_mine(
    State(state): State<AppState>,
    Extension(auth_context): Extension<AuthContext>,
    Query(query): Query<ListWithdrawalsQuery>,
) -> AppResult<Json<Vec<WithdrawalRow>>> {
    let user_id = common::UserId(uuid::Uuid::parse_str(&auth_context.user_id).map_err(|_| AppError::Unauthorized)?);
    let status  = query.status.as_deref().and_then(parse_status);
    let limit   = query.limit.unwrap_or(50).min(200);
    let rows    = state.db.list_withdrawals(Some(user_id), status, limit).await?;
    Ok(Json(rows))
}

pub async fn list_all(
    State(state): State<AppState>,
    Extension(auth_context): Extension<AuthContext>,
    Query(query): Query<ListWithdrawalsQuery>,
) -> AppResult<Json<Vec<WithdrawalRow>>> {
    if !auth_context.is_admin {
        return Err(AppError::Forbidden);
    }
    let status = query.status.as_deref().and_then(parse_status);
    let limit  = query.limit.unwrap_or(100).min(500);
    let rows   = state.db.list_withdrawals(None, status, limit).await?;
    Ok(Json(rows))
}
