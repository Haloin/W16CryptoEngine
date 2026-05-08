use axum::{
    body::Body,
    extract::ConnectInfo,
    http::{Request, Response, StatusCode},
    middleware::Next,
};
use dashmap::DashMap;
use serde_json::json;
use std::{
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

#[derive(Clone)]
struct Window {
    count:      u32,
    window_end: Instant,
}

#[derive(Clone)]
pub struct RateLimiter {
    windows: Arc<DashMap<String, Window>>,
    limit:   u32,
    period:  Duration,
}

impl RateLimiter {
    pub fn new(limit: u32, period: Duration) -> Self {
        let limiter = Self {
            windows: Arc::new(DashMap::new()),
            limit,
            period,
        };
        let windows: Arc<DashMap<String, Window>> = Arc::clone(&limiter.windows);
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            loop {
                interval.tick().await;
                let now = Instant::now();
                windows.retain(|_, w| w.window_end > now);
            }
        });
        limiter
    }

    pub fn check(&self, key: &str) -> bool {
        let now = Instant::now();
        let mut entry = self.windows.entry(key.to_string()).or_insert_with(|| Window {
            count:      0,
            window_end: now + self.period,
        });

        if now > entry.window_end {
            entry.count      = 1;
            entry.window_end = now + self.period;
            return true;
        }

        if entry.count >= self.limit {
            return false;
        }

        entry.count += 1;
        true
    }
}

pub async fn auth_rate_limit(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req:               Request<Body>,
    next:              Next,
) -> Response<Body> {
    static LIMITER: std::sync::OnceLock<RateLimiter> = std::sync::OnceLock::new();
    let limiter = LIMITER.get_or_init(|| RateLimiter::new(10, Duration::from_secs(60)));

    let key = addr.ip().to_string();
    if !limiter.check(&key) {
        let body = serde_json::to_vec(&json!({
            "error": "too many requests",
            "code":  "rate_limited"
        }))
        .unwrap_or_default();

        return Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header("Content-Type", "application/json")
            .header("Retry-After", "60")
            .body(Body::from(body))
            .unwrap_or_default();
    }

    next.run(req).await
}

pub async fn order_rate_limit(
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    req:               Request<Body>,
    next:              Next,
) -> Response<Body> {
    static LIMITER: std::sync::OnceLock<RateLimiter> = std::sync::OnceLock::new();
    let limiter = LIMITER.get_or_init(|| RateLimiter::new(60, Duration::from_secs(1)));

    let key = addr.ip().to_string();
    if !limiter.check(&key) {
        let body = serde_json::to_vec(&json!({
            "error": "order rate limit exceeded",
            "code":  "rate_limited"
        }))
        .unwrap_or_default();

        return Response::builder()
            .status(StatusCode::TOO_MANY_REQUESTS)
            .header("Content-Type", "application/json")
            .header("Retry-After", "1")
            .body(Body::from(body))
            .unwrap_or_default();
    }

    next.run(req).await
}
