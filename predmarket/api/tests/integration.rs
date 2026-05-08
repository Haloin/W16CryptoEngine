use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use serde_json::{json, Value};
use tower::ServiceExt;

mod common;
use common::TestApp;

#[tokio::test]
async fn register_and_login() {
    let app = TestApp::spawn().await;

    let resp = app
        .post("/v1/auth/register", json!({"email": "user@test.com", "password": "password123"}))
        .await;

    assert_eq!(resp.status, StatusCode::CREATED);
    assert!(resp.body["token"].is_string());

    let resp2 = app
        .post("/v1/auth/login", json!({"email": "user@test.com", "password": "password123"}))
        .await;

    assert_eq!(resp2.status, StatusCode::OK);
    assert!(resp2.body["token"].is_string());
}

#[tokio::test]
async fn login_wrong_password_is_401() {
    let app = TestApp::spawn().await;

    app.post("/v1/auth/register", json!({"email": "a@b.com", "password": "correct"}))
        .await;

    let resp = app
        .post("/v1/auth/login", json!({"email": "a@b.com", "password": "wrong"}))
        .await;

    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn duplicate_email_is_conflict() {
    let app  = TestApp::spawn().await;
    let body = json!({"email": "dup@test.com", "password": "password123"});

    let r1 = app.post("/v1/auth/register", body.clone()).await;
    let r2 = app.post("/v1/auth/register", body).await;

    assert_eq!(r1.status, StatusCode::CREATED);
    assert_eq!(r2.status, StatusCode::CONFLICT);
}

#[tokio::test]
async fn balance_requires_auth() {
    let app  = TestApp::spawn().await;
    let resp = app.get_unauthed("/v1/auth/balance").await;
    assert_eq!(resp.status, StatusCode::UNAUTHORIZED);
}

#[tokio::test]
async fn deposit_and_check_balance() {
    let app   = TestApp::spawn().await;
    let admin = app.admin_token().await;
    let user  = app.register_user("bal@test.com", "password123").await;

    let deposit_resp = app
        .post_authed(
            "/v1/admin/deposits",
            json!({
                "user_id":   user.user_id,
                "amount":    100.0,
                "reference": "test-txn-001"
            }),
            &admin,
        )
        .await;

    assert_eq!(deposit_resp.status, StatusCode::CREATED);

    let bal = app.get_authed("/v1/auth/balance", &user.token).await;
    assert_eq!(bal.status, StatusCode::OK);

    let available = bal.body["available"].as_i64().unwrap();
    assert_eq!(available, 1_000_000);
}

#[tokio::test]
async fn create_market_requires_admin() {
    let app  = TestApp::spawn().await;
    let user = app.register_user("nonadmin@test.com", "password123").await;

    let resp = app
        .post_authed(
            "/v1/markets",
            json!({
                "title":       "Test Market",
                "description": "Will it happen?"
            }),
            &user.token,
        )
        .await;

    assert_eq!(resp.status, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn full_order_lifecycle() {
    let app   = TestApp::spawn().await;
    let admin = app.admin_token().await;
    let user  = app.register_user("trader@test.com", "password123").await;

    app.post_authed(
        "/v1/admin/deposits",
        json!({
            "user_id":   user.user_id,
            "amount":    1000.0,
            "reference": "seed-001"
        }),
        &admin,
    )
    .await;

    let market = app
        .post_authed(
            "/v1/markets",
            json!({
                "title":       "BTC above 100k by EOY?",
                "description": "Resolves yes if BTC closes above $100,000 on Dec 31."
            }),
            &admin,
        )
        .await;

    assert_eq!(market.status, StatusCode::CREATED);
    let market_id = market.body["id"].as_str().unwrap();

    let order = app
        .post_authed(
            &format!("/v1/markets/{market_id}/orders"),
            json!({
                "side":     "buy",
                "kind":     "limit",
                "price":    0.65,
                "quantity": 10.0
            }),
            &user.token,
        )
        .await;

    assert_eq!(order.status, StatusCode::CREATED);
    let order_id = order.body["id"].as_str().unwrap();
    assert_eq!(order.body["status"], "open");

    let fetched = app
        .get_authed(&format!("/v1/orders/{order_id}"), &user.token)
        .await;
    assert_eq!(fetched.status, StatusCode::OK);
    assert_eq!(fetched.body["id"], order_id);

    let cancelled = app
        .delete_authed(&format!("/v1/orders/{order_id}"), &user.token)
        .await;
    assert_eq!(cancelled.status, StatusCode::OK);
    assert_eq!(cancelled.body["status"], "cancelled");

    let bal_after = app.get_authed("/v1/auth/balance", &user.token).await;
    let available = bal_after.body["available"].as_i64().unwrap();
    let reserved  = bal_after.body["reserved"].as_i64().unwrap();
    assert_eq!(reserved, 0);
    assert_eq!(available, 10_000_000);
}

#[tokio::test]
async fn idempotent_order_placement() {
    let app   = TestApp::spawn().await;
    let admin = app.admin_token().await;
    let user  = app.register_user("idem@test.com", "password123").await;

    app.post_authed(
        "/v1/admin/deposits",
        json!({"user_id": user.user_id, "amount": 500.0, "reference": "ref-001"}),
        &admin,
    )
    .await;

    let market = app
        .post_authed(
            "/v1/markets",
            json!({"title": "Idem test", "description": "desc"}),
            &admin,
        )
        .await;
    let market_id = market.body["id"].as_str().unwrap();

    let payload = json!({
        "side":     "buy",
        "kind":     "limit",
        "price":    0.5,
        "quantity": 1.0
    });

    let r1 = app
        .post_authed_with_idempotency(
            &format!("/v1/markets/{market_id}/orders"),
            payload.clone(),
            &user.token,
            "key-abc-123",
        )
        .await;

    let r2 = app
        .post_authed_with_idempotency(
            &format!("/v1/markets/{market_id}/orders"),
            payload,
            &user.token,
            "key-abc-123",
        )
        .await;

    assert_eq!(r1.body["id"], r2.body["id"]);
}

#[tokio::test]
async fn order_rejected_when_market_paused() {
    let app   = TestApp::spawn().await;
    let admin = app.admin_token().await;
    let user  = app.register_user("pause@test.com", "password123").await;

    app.post_authed(
        "/v1/admin/deposits",
        json!({"user_id": user.user_id, "amount": 100.0, "reference": "r1"}),
        &admin,
    )
    .await;

    let market = app
        .post_authed(
            "/v1/markets",
            json!({"title": "Pause test", "description": "d"}),
            &admin,
        )
        .await;
    let market_id = market.body["id"].as_str().unwrap();

    let paused = app
        .post_authed(&format!("/v1/markets/{market_id}/pause"), json!({}), &admin)
        .await;
    assert_eq!(paused.status, StatusCode::NO_CONTENT);

    let order = app
        .post_authed(
            &format!("/v1/markets/{market_id}/orders"),
            json!({"side": "buy", "kind": "limit", "price": 0.5, "quantity": 1.0}),
            &user.token,
        )
        .await;

    assert_eq!(order.status, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test]
async fn insufficient_balance_rejected() {
    let app   = TestApp::spawn().await;
    let admin = app.admin_token().await;
    let user  = app.register_user("broke@test.com", "password123").await;

    let market = app
        .post_authed(
            "/v1/markets",
            json!({"title": "Broke test", "description": "d"}),
            &admin,
        )
        .await;
    let market_id = market.body["id"].as_str().unwrap();

    let order = app
        .post_authed(
            &format!("/v1/markets/{market_id}/orders"),
            json!({"side": "buy", "kind": "limit", "price": 0.9, "quantity": 1000.0}),
            &user.token,
        )
        .await;

    assert_eq!(order.status, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(order.body["code"], "insufficient_balance");
}
