use axum::{extract::State, Json};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::state::AppState;
use common::AppResult;

#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub timestamp: String,
    pub components: ComponentStatus,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ComponentStatus {
    pub database: String,
    pub nats: String,
    pub analytics: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReadinessResponse {
    pub ready: bool,
    pub checks: Vec<String>,
}

pub async fn health_check(State(state): State<AppState>) -> AppResult<Json<HealthResponse>> {
    let db_status = check_database(&state).await;
    let nats_status = check_nats(&state).await;
    let analytics_status = "healthy".to_string();

    let status = if db_status == "healthy" && nats_status == "healthy" {
        "healthy"
    } else {
        "degraded"
    };

    Ok(Json(HealthResponse {
        status: status.to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        timestamp: chrono::Utc::now().to_rfc3339(),
        components: ComponentStatus {
            database: db_status,
            nats: nats_status,
            analytics: analytics_status,
        },
    }))
}

pub async fn readiness_check(State(state): State<AppState>) -> AppResult<Json<ReadinessResponse>> {
    let mut checks = Vec::new();
    let mut ready = true;

    if check_database(&state).await == "healthy" {
        checks.push("database: connected".to_string());
    } else {
        checks.push("database: disconnected".to_string());
        ready = false;
    }

    if check_nats(&state).await == "healthy" {
        checks.push("nats: connected".to_string());
    } else {
        checks.push("nats: disconnected".to_string());
        ready = false;
    }

    checks.push("analytics: ready".to_string());

    Ok(Json(ReadinessResponse { ready, checks }))
}

async fn check_database(state: &AppState) -> String {
    match state.db.ping().await {
        Ok(_) => "healthy",
        Err(_) => "unhealthy",
    }
    .to_string()
}

async fn check_nats(state: &AppState) -> String {
    if state.nats.is_connected().await {
        "healthy"
    } else {
        "unhealthy"
    }
    .to_string()
}
