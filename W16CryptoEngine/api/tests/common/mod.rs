use auth::JwtService;
use axum::http::StatusCode;
use cache::MarketCache;
use db::Db;
use markets::MarketService;
use messaging::Nats;
use orders::OrderService;
use positions::FillProcessor;
use secrecy::ExposeSecret;
use settlement::SettlementService;
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;
use axum::body::Body;
use axum::http::{Request, header};

use api_http::{router, state::AppState};

pub struct TestApp {
    pub router: axum::Router,
    pub db:     Arc<Db>,
    pub jwt:    Arc<JwtService>,
}

pub struct TestResponse {
    pub status: StatusCode,
    pub body:   Value,
}

pub struct RegisteredUser {
    pub token:   String,
    pub user_id: String,
}

impl TestApp {
    pub async fn spawn() -> Self {
        let db_url = std::env::var("TEST_DATABASE_URL")
            .unwrap_or_else(|_| "postgres://predmarket:predmarket@localhost/predmarket_test".into());

        let db = Arc::new(
            Db::connect(&db_url, 5, 1, Duration::from_secs(5), Duration::from_secs(30), Duration::from_secs(300))
                .await
                .expect("test db connection failed"),
        );

        let jwt = Arc::new(JwtService::new(b"test-secret-key-not-for-production", 3600));

        let nats_url = std::env::var("TEST_NATS_URL")
            .unwrap_or_else(|_| "nats://localhost:4222".into());

        let nats = Arc::new(
            Nats::connect(
                &nats_url,
                "test.orders".into(),
                "test.fills".into(),
                "test.depth".into(),
                Duration::from_secs(3),
                Duration::from_millis(100),
            )
            .await
            .expect("test nats connection failed"),
        );

        let orders     = Arc::new(OrderService::new(Arc::clone(&db), Arc::clone(&nats), Duration::from_millis(50), Duration::from_millis(50)));
        let markets    = Arc::new(MarketService::new(Arc::clone(&db)));
        let settlement = Arc::new(SettlementService::new(Arc::clone(&db)));
        let cache      = Arc::new(MarketCache::new(Duration::from_secs(5), 1_000));

        let state = AppState { db: Arc::clone(&db), nats, jwt: Arc::clone(&jwt), orders, markets, settlement, cache };

        Self {
            router: router::build(state),
            db,
            jwt,
        }
    }

    pub async fn admin_token(&self) -> String {
        let admin_id = common::UserId::new();
        self.jwt.issue(admin_id, true).expect("jwt issue failed")
    }

    pub async fn register_user(&self, email: &str, password: &str) -> RegisteredUser {
        let resp = self
            .post(
                "/v1/auth/register",
                serde_json::json!({"email": email, "password": password}),
            )
            .await;

        RegisteredUser {
            token:   resp.body["token"].as_str().unwrap().to_string(),
            user_id: resp.body["user_id"].as_str().unwrap().to_string(),
        }
    }

    pub async fn post(&self, path: &str, body: Value) -> TestResponse {
        self.request(
            Request::builder()
                .method("POST")
                .uri(path)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
    }

    pub async fn get_unauthed(&self, path: &str) -> TestResponse {
        self.request(
            Request::builder()
                .method("GET")
                .uri(path)
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    pub async fn get_authed(&self, path: &str, token: &str) -> TestResponse {
        self.request(
            Request::builder()
                .method("GET")
                .uri(path)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    pub async fn post_authed(&self, path: &str, body: Value, token: &str) -> TestResponse {
        self.request(
            Request::builder()
                .method("POST")
                .uri(path)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
    }

    pub async fn post_authed_with_idempotency(
        &self,
        path:     &str,
        body:     Value,
        token:    &str,
        idem_key: &str,
    ) -> TestResponse {
        self.request(
            Request::builder()
                .method("POST")
                .uri(path)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .header(header::CONTENT_TYPE, "application/json")
                .header("idempotency-key", idem_key)
                .body(Body::from(serde_json::to_vec(&body).unwrap()))
                .unwrap(),
        )
        .await
    }

    pub async fn delete_authed(&self, path: &str, token: &str) -> TestResponse {
        self.request(
            Request::builder()
                .method("DELETE")
                .uri(path)
                .header(header::AUTHORIZATION, format!("Bearer {token}"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
    }

    async fn request(&self, req: Request<Body>) -> TestResponse {
        let response = self.router.clone().oneshot(req).await.unwrap();
        let status   = response.status();
        let bytes    = axum::body::to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let body     = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        TestResponse { status, body }
    }
}
