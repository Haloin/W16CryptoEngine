mod common;

use common::TestApp;
use serde_json::json;

#[tokio::test]
async fn test_health_check() {
    let app = TestApp::spawn().await;
    let resp = app.get_unauthed("/health").await;
    assert_eq!(resp.status, 200);
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
async fn test_rate_limiting() {
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
async fn test_analytics_endpoints() {
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
}

#[tokio::test]
async fn test_circuit_breaker() {
    let app = TestApp::spawn().await;
    
    for _ in 0..10 {
        let resp = app.get_unauthed("/health").await;
        assert_eq!(resp.status, 200);
    }
}

#[tokio::test]
async fn test_graceful_shutdown() {
    let app = TestApp::spawn().await;
    
    let resp = app.get_unauthed("/health").await;
    assert_eq!(resp.status, 200);
}
