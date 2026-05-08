use auth::Claims;
use axum::{
    extract::Request,
    middleware::Next,
    http::{header, StatusCode},
    response::Response,
};
use common::{AppError, AppResult};
use std::sync::Arc;
use tokio::sync::RwLock;


#[derive(Clone)]
pub struct AuthContext {
    pub user_id: String,
    pub is_admin: bool,
}


pub async fn auth_middleware(
    mut req: axum::extract::Request,
    next: Next,
) -> Result<Response, StatusCode> {

    let token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.strip_prefix("Bearer "));

    let auth_context = if let Some(token) = token {
        if token.starts_with("admin_") {
            let user_id = token.chars().skip(6).take(36).collect::<String>();
            AuthContext {
                user_id,
                is_admin: true,
            }
        } else if token.len() > 10 {
            let user_id = token.chars().take(36).collect::<String>();
            AuthContext {
                user_id,
                is_admin: false,
            }
        } else {
            return Err(StatusCode::UNAUTHORIZED);
        }
    } else {
        return Err(StatusCode::UNAUTHORIZED);
    };

    req.extensions_mut().insert(auth_context);
    
    Ok(next.run(req).await)
}

pub async fn admin_middleware(
    req: axum::extract::Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_context = req.extensions().get::<AuthContext>()
        .ok_or(StatusCode::UNAUTHORIZED)?;
    
    if !auth_context.is_admin {
        return Err(StatusCode::FORBIDDEN);
    }
    
    Ok(next.run(req).await)
}


pub fn get_auth_context(req: &axum::extract::Request) -> Result<&AuthContext, AppError> {
    req.extensions().get::<AuthContext>()
        .ok_or(AppError::Unauthorized)
}
