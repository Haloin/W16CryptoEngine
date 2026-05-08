use axum::{
    extract::{Path, State, Request},
    Json,
};
use common::{AppError, AppResult, MarketId, Position, Signal, SignalId, UserId};
use analytics::{SignalStrength, TradingSignalType, PnLTracker, RiskManager};
use crate::middleware::auth::get_auth_context;
use serde_json::json;
use uuid::Uuid;

use crate::state::AppState;

pub async fn get_signals(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let market_id = MarketId(id);
    let signals = state.analytics.generate_signals(market_id).await;

    let signal_json: Vec<serde_json::Value> = signals
        .into_iter()
        .map(|s| {
            json!({
                "market_id": s.market_id,
                "type": format!("{:?}", s.signal_type),
                "strength": format!("{:?}", s.strength),
                "confidence": s.confidence,
                "expected_return": s.expected_return,
                "time_horizon": s.time_horizon,
                "created_at": s.created_at.to_rfc3339(),
                "expires_at": s.expires_at.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({ "signals": signal_json })))
}

pub async fn get_market_regime(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let market_id = MarketId(id);
    let regime = state.analytics.current_regime(market_id).await;

    Ok(Json(json!({
        "market_id": id.to_string(),
        "regime": format!("{:?}", regime),
    })))
}

pub async fn get_regime_details(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let market_id = MarketId(id);

    let state_opt = state.analytics.regime_detector.get_regime_state(market_id).await;

    match state_opt {
        Some(r) => Ok(Json(json!({
            "market_id": r.market_id,
            "regime": format!("{:?}", r.regime),
            "volatility": format!("{:?}", r.volatility),
            "trend_strength": r.trend_strength,
            "volatility_measure": r.volatility_measure,
            "liquidity_score": r.liquidity_score,
            "timestamp": r.timestamp.to_rfc3339(),
        }))),
        None => Ok(Json(json!({"error": "insufficient data"}))),
    }
}

pub async fn get_position_alerts(
    State(state): State<AppState>,
) -> AppResult<Json<serde_json::Value>> {
    let alerts: Vec<String> = vec![];

    let alert_json: Vec<serde_json::Value> = alerts
        .into_iter()
        .map(|a| {
            json!({
                "alert": a,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({ "alerts": alert_json })))
}

pub async fn get_ml_prediction(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> AppResult<Json<serde_json::Value>> {
    let market_id = MarketId(id);

    match state.analytics.predict(market_id).await {
        Some(pred) => Ok(Json(json!({
            "market_id": pred.market_id,
            "signal_type": if pred.predicted_direction > 0.0 { TradingSignalType::Buy } else { TradingSignalType::Sell },
            "confidence": pred.confidence,
            "expected_return_bps": pred.expected_return_bps,
            "volatility_forecast": pred.volatility_forecast,
            "features": pred.features,
            "model_version": pred.model_version,
            "timestamp": pred.timestamp.to_rfc3339(),
        }))),
        None => Ok(Json(json!({"error": "insufficient data for prediction"}))),
    }
}

pub async fn get_arbitrage_opportunities(
    State(state): State<AppState>,
) -> AppResult<Json<serde_json::Value>> {
    let opportunities = state.analytics.scan_arbitrage().await;

    let opp_json: Vec<serde_json::Value> = opportunities
        .into_iter()
        .map(|o| {
            json!({
                "market_a": o.market_a,
                "market_b": o.market_b,
                "spread_zscore": o.spread_zscore,
                "correlation": o.correlation,
                "hedge_ratio": o.hedge_ratio,
                "action": format!("{:?}", o.suggested_action),
                "expected_profit_bps": o.expected_profit_bps,
                "confidence": o.confidence,
                "timestamp": o.timestamp.to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({ "opportunities": opp_json })))
}

pub async fn get_pnl(
    State(state): State<AppState>,
    req: Request,
) -> AppResult<Json<serde_json::Value>> {
    let auth_context = get_auth_context(&req).map_err(|_| AppError::Unauthorized)?;
    let user_uuid = uuid::Uuid::parse_str(&auth_context.user_id)?;
    let user_id = UserId(user_uuid);
    let (realized, unrealized) = state.analytics.get_user_pnl(user_id).await;
    let exposure = state.analytics.get_user_exposure(user_id).await;

    Ok(Json(json!({
        "user_id": auth_context.user_id,
        "realized_pnl": realized,
        "unrealized_pnl": unrealized,
        "total_pnl": realized + unrealized,
        "exposure": exposure,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    })))
}

pub async fn get_risk(
    State(state): State<AppState>,
    auth: AuthContextExtractor,
) -> AppResult<Json<serde_json::Value>> {
    let user_uuid = uuid::Uuid::parse_str(&auth.0.user_id)?;
    let user_id = UserId(user_uuid);
    let exposure = state.analytics.get_user_exposure(user_id).await;
    let (realized, unrealized) = state.analytics.get_user_pnl(user_id).await;
    let total_pnl = realized + unrealized;
    let alerts: Vec<String> = vec![];

    let risk_score = if exposure > 100_000.0 { "high" } else { "normal" };

    Ok(Json(json!({
        "user_id": auth.0.user_id,
        "exposure_usd": exposure,
        "realized_pnl": realized,
        "unrealized_pnl": unrealized,
        "total_pnl": total_pnl,
        "risk_level": risk_score,
        "alerts": alerts,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    })))
}

pub async fn get_positions(
    State(state): State<AppState>,
    auth: AuthContextExtractor,
) -> AppResult<Json<serde_json::Value>> {
    let user_uuid = uuid::Uuid::parse_str(&auth.0.user_id)?;
    let user_id = UserId(user_uuid);
    let alerts: Vec<String> = vec![];
    let exposure = state.analytics.get_user_exposure(user_id).await;
    let (realized, unrealized) = state.analytics.get_user_pnl(user_id).await;

    let alert_json: Vec<serde_json::Value> = alerts
        .into_iter()
        .map(|a| {
            json!({
                "alert": a,
                "timestamp": chrono::Utc::now().to_rfc3339(),
            })
        })
        .collect();

    Ok(Json(json!({
        "user_id": auth.0.user_id,
        "positions": alert_json,
        "exposure_usd": exposure,
        "realized_pnl": realized,
        "unrealized_pnl": unrealized,
        "timestamp": chrono::Utc::now().to_rfc3339(),
    })))
}
