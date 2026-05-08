use axum::{
    body::Body,
    http::{Request, Response},
    middleware::Next,
};
use prometheus::{Counter, Histogram, HistogramOpts, IntCounterVec, Opts, Registry};
use std::sync::OnceLock;
use std::time::Instant;

static REGISTRY: OnceLock<Registry> = OnceLock::new();
static HTTP_REQUESTS: OnceLock<IntCounterVec> = OnceLock::new();
static HTTP_DURATION: OnceLock<Histogram> = OnceLock::new();

pub fn init_metrics() -> &'static Registry {
    let registry = REGISTRY.get_or_init(Registry::new);

    HTTP_REQUESTS.get_or_init(|| {
        let counter = IntCounterVec::new(
            Opts::new("http_requests_total", "Total HTTP requests"),
            &["method", "path", "status"],
        )
        .unwrap_or_default();
        registry.register(Box::new(counter.clone())).ok();
        counter
    });

    HTTP_DURATION.get_or_init(|| {
        let hist = Histogram::with_opts(
            HistogramOpts::new("http_request_duration_seconds", "HTTP request duration")
                .buckets(vec![0.001, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0]),
        )
        .unwrap_or_default();
        registry.register(Box::new(hist.clone())).ok();
        hist
    });

    registry
}

pub fn gather_metrics() -> String {
    use prometheus::Encoder;
    let encoder = prometheus::TextEncoder::new();
    let mut buf = Vec::new();
    let registry = REGISTRY.get_or_init(Registry::new);
    encoder.encode(&registry.gather(), &mut buf).unwrap_or(());
    String::from_utf8(buf).unwrap_or_default()
}

pub async fn track(req: Request<Body>, next: Next) -> Response<Body> {
    let method = req.method().to_string();
    let path = req
        .uri()
        .path()
        .split('/')
        .enumerate()
        .map(|(i, seg)| {
            if i > 0 && seg.len() == 36 && seg.contains('-') {
                ":id"
            } else {
                seg
            }
        })
        .collect::<Vec<_>>()
        .join("/");

    let start = Instant::now();
    let response = next.run(req).await;
    let elapsed = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    if let Some(counter) = HTTP_REQUESTS.get() {
        counter.with_label_values(&[&method, &path, &status]).inc();
    }
    if let Some(hist) = HTTP_DURATION.get() {
        hist.observe(elapsed);
    }

    response
}
