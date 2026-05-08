use axum::{
    middleware,
    routing::{delete, get, post},
    Router,
};
use std::time::Duration;
use tower_http::{
    cors::{Any, CorsLayer},
    request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

use crate::{
    handlers::{analytics as analytics_handlers, auth as auth_handlers, deposits, health, markets, orders, withdrawals, ws},
    middleware::{auth as auth_middleware, metrics, rate_limit},
    state::AppState,
};

pub fn build(state: AppState) -> Router {
    let req_id_header = axum::http::HeaderName::from_static("x-request-id");

    let auth_routes = Router::new()
        .route("/auth/register", post(auth_handlers::register))
        .route("/auth/login",    post(auth_handlers::login))
        .layer(middleware::from_fn(rate_limit::auth_rate_limit));

    let user_routes = Router::new()
        .route("/auth/balance",             get(auth_handlers::balance))
        .route("/orders",                   get(orders::list_orders))
        .route("/orders/:id",               get(orders::get_order))
        .route("/orders/:id",               delete(orders::cancel_order))
        .route("/markets/:id/orders",       post(orders::place_order))
        .route("/withdrawals",              post(withdrawals::request))
        .route("/withdrawals",              get(withdrawals::list_mine))
        .route("/analytics/signals",        get(analytics_handlers::get_signals))
        .route("/analytics/pnl",            get(analytics_handlers::get_pnl))
        .route("/analytics/risk",           get(analytics_handlers::get_risk))
        .route("/analytics/positions",    get(analytics_handlers::get_positions))
        .layer(middleware::from_fn(auth_middleware::auth_middleware))
        .layer(middleware::from_fn(rate_limit::order_rate_limit));

    let admin_routes = Router::new()
        .route("/admin/deposits",                   post(deposits::deposit))
        .route("/admin/withdrawals",                get(withdrawals::list_all))
        .route("/admin/withdrawals/:id/approve",    post(withdrawals::approve))
        .route("/admin/withdrawals/:id/reject",     post(withdrawals::reject))
        .layer(middleware::from_fn(auth_middleware::admin_middleware));

    let public_routes = Router::new()
        .route("/health", get(health::health_check))
        .route("/ready", get(health::readiness_check))
        .route("/markets",             get(markets::list_markets))
        .route("/markets",             post(markets::create_market))
        .route("/markets/:id",         get(markets::get_market))
        .route("/metrics", get(|| async { metrics::gather_metrics() }));

    let ws_routes = Router::new()
        .route("/markets/:id/ws", get(ws::market_depth));

    let api = Router::new()
        .merge(auth_routes)
        .merge(user_routes)
        .merge(admin_routes)
        .merge(public_routes)
        .merge(ws_routes)
        .layer(middleware::from_fn(metrics::track));

    Router::new()
        .nest("/v1", api)
        .route("/healthz", get(healthz))
        .route("/metrics", get(get_metrics))
        .layer(
            tower::ServiceBuilder::new()
                .layer(SetRequestIdLayer::new(req_id_header.clone(), MakeRequestUuid))
                .layer(PropagateRequestIdLayer::new(req_id_header))
                .layer(TraceLayer::new_for_http())
                .layer(TimeoutLayer::new(Duration::from_secs(30)))
                .layer(
                    CorsLayer::new()
                        .allow_origin(Any)
                        .allow_methods(Any)
                        .allow_headers(Any),
                ),
        )
        .with_state(state)
}

async fn healthz() -> axum::http::StatusCode {
    axum::http::StatusCode::OK
}

async fn get_metrics() -> String {
    metrics::gather_metrics()
}
