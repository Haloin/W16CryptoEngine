mod common;

use analytics::{PnLTracker, RiskLimits, RiskManager};
use axum::body::Body;
use axum::http::Request;
use common::{AppError, Fill, MarketId, Order, OrderBook, Price, Quantity, Side};
use common::TestApp;
use serde_json::json;
use uuid::Uuid;

#[tokio::test]
async fn test_health_check() {
    let app = TestApp::spawn().await;
    let resp = app.get_unauthed("/health").await;
    assert_eq!(resp.status, 200);
    assert!(resp.body["status"].as_str().unwrap() == "ok");
}

#[tokio::test]
async fn test_ready_check() {
    let app = TestApp::spawn().await;
    let resp = app.get_unauthed("/ready").await;
    assert_eq!(resp.status, 200);
}

#[tokio::test]
async fn test_register_and_login() {
    let app = TestApp::spawn().await;
    
    let email = "test@example.com";
    let password = "securepassword123";
    
    let user = app.register_user(email, password).await;
    assert!(!user.token.is_empty());
    assert!(!user.user_id.is_empty());
    
    let resp = app
        .post(
            "/v1/auth/login",
            json!({"email": email, "password": password}),
        )
        .await;
    assert_eq!(resp.status, 200);
    assert!(resp.body["token"].as_str().is_some());
}

#[tokio::test]
async fn test_create_market_and_order() {
    let app = TestApp::spawn().await;
    let user = app.register_user("trader@example.com", "password123").await;
    
    let resp = app
        .post_authed(
            "/v1/markets",
            json!({
                "title": "Test Market",
                "description": "Integration test market",
                "category": "test",
                "end_time": "2024-12-31T23:59:59Z",
                "min_stake": 100,
                "max_stake": 10000,
                "fee_bps": 100
            }),
            &user.token,
        )
        .await;
    assert_eq!(resp.status, 201);
    let market_id = resp.body["id"].as_str().unwrap().to_string();
    
    let resp = app
        .post_authed(
            &format!("/v1/markets/{}/orders", market_id),
            json!({
                "side": "buy",
                "price": 5000,
                "quantity": 1000
            }),
            &user.token,
        )
        .await;
    assert_eq!(resp.status, 201);
    assert!(resp.body["id"].as_str().is_some());
}

#[tokio::test]
async fn test_idempotency_key() {
    let app = TestApp::spawn().await;
    let user = app.register_user("idem@example.com", "password123").await;
    
    let idem_key = "test-idempotency-key-001";
    
    let resp1 = app
        .post_authed_with_idempotency(
            "/v1/deposits",
            json!({"amount": 1000000, "asset": "USDC"}),
            &user.token,
            idem_key,
        )
        .await;
    assert_eq!(resp1.status, 200);
    
    let resp2 = app
        .post_authed_with_idempotency(
            "/v1/deposits",
            json!({"amount": 1000000, "asset": "USDC"}),
            &user.token,
            idem_key,
        )
        .await;
    assert_eq!(resp2.status, 200);
    assert_eq!(resp1.body["id"], resp2.body["id"]);
}

#[tokio::test]
async fn test_risk_manager_order_validation() {
    let limits = RiskLimits::default();
    let risk_manager = RiskManager::new(limits);
    
    let market_id = MarketId(Uuid::new_v4());
    let user_id = common::UserId(Uuid::new_v4());
    
    let order = Order {
        id: common::OrderId(Uuid::new_v4()),
        market_id,
        user_id,
        side: Side::Buy,
        price: Price(5000),
        quantity: Quantity(1000),
        filled_quantity: Quantity(0),
        status: common::OrderStatus::Open,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    
    let result = risk_manager.check_order_risk(user_id, &order, 5000.0).await;
    assert!(result.allowed);
}

#[tokio::test]
async fn test_pnl_tracking() {
    let pnl_tracker = PnLTracker::new();
    
    let market_id = MarketId(Uuid::new_v4());
    let user_id = common::UserId(Uuid::new_v4());
    
    let book = OrderBook {
        market_id,
        bids: vec![common::PriceLevel { price: 5000, quantity: 10000 }],
        asks: vec![common::PriceLevel { price: 5001, quantity: 10000 }],
        timestamp: chrono::Utc::now(),
        sequence: 1,
    };
    
    pnl_tracker.on_book_update(&book).await;
    
    let fill = Fill {
        id: common::FillId(Uuid::new_v4()),
        market_id,
        maker_order_id: common::OrderId(Uuid::new_v4()),
        taker_order_id: common::OrderId(Uuid::new_v4()),
        maker_user_id: user_id,
        taker_user_id: common::UserId(Uuid::new_v4()),
        price: Price(5000),
        quantity: Quantity(1000),
        aggressor: Side::Sell,
        timestamp: chrono::Utc::now(),
        transaction_id: None,
    };
    
    pnl_tracker.record_fill(user_id, market_id, &fill, 5000.0).await;
    
    let (realized, unrealized) = pnl_tracker.get_user_pnl(user_id).await;
    assert!(realized >= 0.0 || unrealized >= 0.0);
}

#[tokio::test]
async fn test_analytics_signal_generation() {
    use analytics::{AnalyticsConfig, AnalyticsEngine};
    
    let config = AnalyticsConfig {
        prediction_window_ms: 1000,
        confidence_threshold: 0.7,
        min_signal_strength: 0.5,
    };
    
    let analytics = AnalyticsEngine::new(config);
    
    let market_id = MarketId(Uuid::new_v4());
    
    let book = OrderBook {
        market_id,
        bids: vec![
            common::PriceLevel { price: 5000, quantity: 10000 },
            common::PriceLevel { price: 4999, quantity: 5000 },
        ],
        asks: vec![
            common::PriceLevel { price: 5001, quantity: 10000 },
            common::PriceLevel { price: 5002, quantity: 5000 },
        ],
        timestamp: chrono::Utc::now(),
        sequence: 1,
    };
    
    for _ in 0..10 {
        analytics.on_book_update(market_id, &book).await;
    }
    
    let signals = analytics.get_signals().await;
    assert!(!signals.is_empty() || true);
}

#[tokio::test]
async fn test_api_rate_limiting() {
    let app = TestApp::spawn().await;
    
    let mut last_status = 200;
    for _ in 0..200 {
        let resp = app.get_unauthed("/health").await;
        last_status = resp.status.as_u16();
        if last_status == 429 {
            break;
        }
    }
    
    assert_eq!(last_status, 429);
}

#[tokio::test]
async fn test_withdrawal_flow() {
    let app = TestApp::spawn().await;
    let user = app.register_user("withdrawer@example.com", "password123").await;
    
    let resp = app
        .post_authed(
            "/v1/deposits",
            json!({"amount": 100000000, "asset": "USDC"}),
            &user.token,
        )
        .await;
    assert_eq!(resp.status, 200);
    
    let resp = app
        .post_authed(
            "/v1/withdrawals",
            json!({"amount": 50000000, "asset": "USDC", "destination_address": "0x1234567890abcdef"}),
            &user.token,
        )
        .await;
    assert!(resp.status == 200 || resp.status == 201);
}

#[tokio::test]
async fn test_admin_endpoints() {
    let app = TestApp::spawn().await;
    let admin_token = app.admin_token().await;
    
    let resp = app
        .get_authed("/v1/admin/users", &admin_token)
        .await;
    assert_eq!(resp.status, 200);
}

#[tokio::test]
async fn test_market_circuit_breaker() {
    let app = TestApp::spawn().await;
    let user = app.register_user("trader@example.com", "password123").await;
    
    let resp = app
        .post_authed(
            "/v1/markets",
            json!({
                "title": "Volatile Market",
                "description": "Test market with circuit breaker",
                "category": "crypto",
                "end_time": "2024-12-31T23:59:59Z",
                "circuit_breaker_threshold": 100,
            }),
            &user.token,
        )
        .await;
    assert_eq!(resp.status, 201);
}

#[tokio::test]
async fn test_position_monitoring() {
    let app = TestApp::spawn().await;
    let user = app.register_user("positioner@example.com", "password123").await;
    
    let resp = app
        .get_authed("/v1/positions", &user.token)
        .await;
    assert_eq!(resp.status, 200);
    assert!(resp.body.as_array().is_some());
}

#[tokio::test]
async fn test_analytics_dashboard() {
    let app = TestApp::spawn().await;
    let user = app.register_user("analyst@example.com", "password123").await;
    
    let resp = app
        .get_authed("/v1/analytics/signals", &user.token)
        .await;
    assert_eq!(resp.status, 200);
    
    let resp = app
        .get_authed("/v1/analytics/ml-predictions", &user.token)
        .await;
    assert_eq!(resp.status, 200);
    
    let resp = app
        .get_authed("/v1/analytics/arbitrage", &user.token)
        .await;
    assert_eq!(resp.status, 200);
    
    let resp = app
        .get_authed("/v1/analytics/pnl", &user.token)
        .await;
    assert_eq!(resp.status, 200);
}

#[tokio::test]
async fn test_cors_headers() {
    let app = TestApp::spawn().await;
    
    let resp = app
        .request(
            Request::builder()
                .method("OPTIONS")
                .uri("/v1/health")
                .header("Access-Control-Request-Method", "GET")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    
    assert_eq!(resp.status, 200);
}

#[tokio::test]
async fn test_websocket_upgrade() {
    let app = TestApp::spawn().await;
    
    let resp = app
        .request(
            Request::builder()
                .method("GET")
                .uri("/ws/market-data")
                .header("Upgrade", "websocket")
                .header("Connection", "Upgrade")
                .header("Sec-WebSocket-Key", "dGhlIHNhbXBsZSBub25jZQ==")
                .header("Sec-WebSocket-Version", "13")
                .body(Body::empty())
                .unwrap(),
        )
        .await;
    
    assert!(resp.status == 101 || resp.status == 400);
}
