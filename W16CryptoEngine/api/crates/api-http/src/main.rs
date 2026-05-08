use analytics::{AnalyticsEngine, RiskLimits, RiskManager};
use audit::AuditService;
use auth::JwtService;
use cache::MarketCache;
use common::{Config, AppError};
use db::Db;
use markets::MarketService;
use messaging::Nats;
use orders::{OrderService, reconciliation::OrderReconciler};
use positions::FillProcessor;
use secrecy::ExposeSecret;
use settlement::SettlementService;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tracing::{error, info, warn};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

mod handlers;
mod middleware;
mod router;
mod state;
mod trading_engine;
mod blockchain;

use state::AppState;
use trading_engine::{TradingEngine, TradingEngineStatus};
use blockchain::{Web3Provider, TransactionManager, ChainId};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .with(fmt::layer().json())
        .init();

    let config = Config::load()?;

    middleware::metrics::init_metrics();

    let db = Arc::new(
        Db::connect(
            config.database.url.expose_secret(),
            config.database.max_connections,
            config.database.min_connections,
            config.db_connect_timeout(),
            config.db_idle_timeout(),
            config.db_max_lifetime(),
        )
        .await?,
    );

    let nats = Arc::new(
        Nats::connect(
            &config.nats.url,
            config.nats.order_subject.clone(),
            config.nats.fill_subject.clone(),
            config.nats.depth_subject.clone(),
            config.nats_connect_timeout(),
            Duration::from_millis(config.nats.request_timeout),
        )
        .await?,
    );

    let jwt = Arc::new(JwtService::new(
        config.jwt.secret.expose_secret().as_bytes(),
        config.jwt.ttl_secs,
    ));

    let risk_limits = RiskLimits {
        max_position_size_usd: 1_000_000.0,
        max_daily_loss_usd: 100_000.0,
        max_total_exposure_usd: 2_000_000.0,
        max_drawdown_pct: 0.05,
        max_positions_per_market: 5,
        max_correlated_exposure: 500_000.0,
    };

    let polymarket_client = Arc::new(polymarket::PolymarketClient::new(
        config.polymarket_api_key.clone().unwrap_or_default(),
        config.polymarket_api_secret.clone().unwrap_or_default(),
        config.polymarket_api_passphrase.clone().unwrap_or_default(),
    ).map_err(|e| {
        error!(error = %e, "Initialization failure: PolymarketClient");
        e
    })?);

    let orders = Arc::new(OrderService::new(
        Arc::new(RiskManager::new(risk_limits.clone())),
        Arc::new(OrderReconciler::new()),
        Arc::clone(&polymarket_client),
    ));

    let markets = Arc::new(MarketService::new(Arc::clone(&db)));
    let settlement = Arc::new(SettlementService::new(Arc::clone(&db)));
    let cache = Arc::new(MarketCache::new(Duration::from_secs(30), 10_000));

    let fill_processor = Arc::new(FillProcessor::new(Arc::clone(&db), Arc::clone(&nats)));
    fill_processor.spawn();

    let analytics = Arc::new(AnalyticsEngine::new(Arc::clone(&db)));
    spawn_analytics_worker(Arc::clone(&analytics), Arc::clone(&nats));

    let state = AppState {
        db:         Arc::clone(&db),
        nats:       Arc::clone(&nats),
        jwt:        Arc::clone(&jwt),
        orders:     Arc::clone(&orders),
        markets:    Arc::clone(&markets),
        settlement: Arc::clone(&settlement),
        cache:      Arc::clone(&cache),
        analytics:  Arc::clone(&analytics),
    };

    let trading_engine = Arc::new(TradingEngine::new(
        state,
        Arc::clone(&analytics),
    ));
    let mut trading_engine_handle: Option<tokio::task::JoinHandle<()>> = None;

    if let (Some(api_key), Some(api_secret), Some(api_pass)) = (
        config.polymarket_api_key.as_ref(),
        config.polymarket_api_secret.as_ref(),
        config.polymarket_api_passphrase.as_ref(),
    ) {
        trading_engine.initialize(
            api_key.clone(),
            api_secret.clone(),
            api_pass.clone(),
        ).await?;
        
        let market_ids = config.trading_markets.clone().unwrap_or_default();
        trading_engine.start(market_ids).await?;
        trading_engine_handle = Some(trading_engine.spawn_graceful_shutdown_monitor());
        info!("Trading engine started with Polymarket integration");
    } else {
        warn!("Polymarket credentials not configured, trading engine disabled");
    }

    let blockchain_provider = if let (Some(rpc_url), Some(private_key)) = (
        config.polygon_rpc_url.as_ref(),
        config.polygon_private_key.as_ref(),
    ) {
        match Web3Provider::new(
            rpc_url,
            private_key,
            ChainId::Polygon,
        ).await {
            Ok(provider) => {
                info!("Web3 provider initialized for Polygon");
                let provider = Arc::new(provider);
                let tx_manager = Arc::new(TransactionManager::new(Arc::clone(&provider)));
                Some((provider, tx_manager))
            }
            Err(e) => {
                error!("Failed to initialize Web3 provider: {}", e);
                None
            }
        }
    } else {
        warn!("Blockchain credentials not configured, Web3 provider disabled");
        None
    };

    spawn_cleanup_job(Arc::clone(&db));

    let app      = router::build(state);
    let addr     = config.server_addr();
    let listener = tokio::net::TcpListener::bind(&addr).await?;

    info!(addr, "server listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal(
            config.server.shutdown_timeout,
            trading_engine,
            trading_engine_handle,
        ))
        .await?;

    info!("server stopped");
    Ok(())
}

fn spawn_cleanup_job(db: Arc<Db>) {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(3600));
        loop {
            interval.tick().await;
            match db.expire_idempotency_keys().await {
                Ok(n)  => { if n > 0 { info!(count = n, "expired idempotency keys"); } }
                Err(e) => warn!(error = %e, "idempotency cleanup failed"),
            }
        }
    });
}

async fn shutdown_signal(
    timeout_secs: u64,
    trading_engine: Arc<TradingEngine>,
    _trading_handle: Option<tokio::task::JoinHandle<()>>,
) {
    let ctrl_c = async {
        match signal::ctrl_c().await {
            Ok(()) => {},
            Err(e) => {
                error!(error = %e, "SigInt handler installation failed");
            }
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut signal) => {
                signal.recv().await;
            }
            Err(e) => {
                error!(error = %e, "SigTerm handler installation failed");
                std::future::pending::<()>().await;
            }
        }
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c    => {},
        _ = terminate => {},
    }

    info!(timeout_secs, "shutdown initiated, draining connections");
    
    trading_engine.stop().await;
    info!("engine offline");
    
    tokio::time::sleep(Duration::from_secs(timeout_secs)).await;
}

fn spawn_analytics_worker(
    analytics: Arc<AnalyticsEngine>,
    _nats: Arc<Nats>,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            
            let regime = analytics.current_regime(common::MarketId(uuid::Uuid::nil())).await;
            info!(regime = ?regime, "Market regime updated");
        }
    })
}
