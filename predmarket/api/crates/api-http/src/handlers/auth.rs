use auth::PasswordService;
use axum::{extract::State, Json};
use common::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use crate::{middleware::auth::get_auth_context, state::AppState};

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterRequest {
    pub email:    String,
    pub password: String,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct LoginRequest {
    pub email:    String,
    pub password: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct TokenResponse {
    pub token:    String,
    pub user_id:  String,
    pub is_admin: bool,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct BalanceResponse {
    pub available: i64,
    pub reserved:  i64,
}

#[utoipa::path(post, path = "/auth/register", request_body = RegisterRequest, responses((status = 201, body = TokenResponse)))]
pub async fn register(
    State(state): State<AppState>,
    Json(payload):    Json<RegisterRequest>,
) -> AppResult<(axum::http::StatusCode, Json<TokenResponse>)> {
    if payload.email.is_empty() || !payload.email.contains('@') {
        return Err(AppError::Validation("invalid email".into()));
    }
    if payload.password.len() < 8 {
        return Err(AppError::Validation("password must be at least 8 characters".into()));
    }

    let hash    = PasswordService::hash(&payload.password)?;
    let user_id = state.db.insert_user(&payload.email, &hash).await?;
    state.db.credit_balance(user_id, 0).await?;
    let token   = state.jwt.issue(user_id, false)?;

    Ok((
        axum::http::StatusCode::CREATED,
        Json(TokenResponse {
            token,
            user_id:  user_id.to_string(),
            is_admin: false,
        }),
    ))
}

#[utoipa::path(post, path = "/auth/login", request_body = LoginRequest, responses((status = 200, body = TokenResponse)))]
pub async fn login(
    State(state): State<AppState>,
    Json(payload):    Json<LoginRequest>,
) -> AppResult<Json<TokenResponse>> {
    let user = state
        .db
        .get_user_by_email(&payload.email)
        .await
        .map_err(|_| AppError::Unauthorized)?;

    PasswordService::verify(&payload.password, &user.password_hash)?;

    let user_id = common::UserId(user.id);
    let token   = state.jwt.issue(user_id, user.is_admin)?;

    Ok(Json(TokenResponse {
        token,
        user_id:  user_id.to_string(),
        is_admin: user.is_admin,
    }))
}

#[utoipa::path(get, path = "/auth/balance", responses((status = 200, body = BalanceResponse)))]
pub async fn balance(
    State(state): State<AppState>,
    Extension(auth_context): Extension<AuthContext>,
) -> AppResult<Json<BalanceResponse>> {
    let user_id = common::UserId(uuid::Uuid::parse_str(&auth_context.user_id).map_err(|_| AppError::Unauthorized)?);
    let bal     = state.db.get_balance(user_id).await?;
    Ok(Json(BalanceResponse {
        available: bal.available,
        reserved:  bal.reserved,
    }))
}
