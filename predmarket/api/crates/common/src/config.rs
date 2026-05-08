use secrecy::SecretString;
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Deserialize)]
pub struct Config {
    pub server:   ServerConfig,
    pub database: DatabaseConfig,
    pub nats:     NatsConfig,
    pub jwt:      JwtConfig,
    pub engine:   EngineConfig,
    
    pub polymarket_api_key:        Option<String>,
    pub polymarket_api_secret:     Option<String>,
    pub polymarket_api_passphrase: Option<String>,
    pub trading_markets:           Option<Vec<String>>,
    
    pub polygon_rpc_url:           Option<String>,
    pub polygon_private_key:       Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct ServerConfig {
    pub host:            String,
    pub port:            u16,
    pub shutdown_timeout: u64,
}

#[derive(Debug, Deserialize)]
pub struct DatabaseConfig {
    pub url:              SecretString,
    pub max_connections:  u32,
    pub min_connections:  u32,
    pub connect_timeout:  u64,
    pub idle_timeout:     u64,
    pub max_lifetime:     u64,
}

#[derive(Debug, Deserialize)]
pub struct NatsConfig {
    pub url:               String,
    pub order_subject:     String,
    pub fill_subject:      String,
    pub depth_subject:     String,
    pub connect_timeout:   u64,
    pub request_timeout:   u64,
}

#[derive(Debug, Deserialize)]
pub struct JwtConfig {
    pub secret:   SecretString,
    pub ttl_secs: i64,
}

#[derive(Debug, Deserialize)]
pub struct EngineConfig {
    pub submit_timeout_ms:   u64,
    pub cancel_timeout_ms:   u64,
    pub cb_failure_threshold: u32,
    pub cb_recovery_secs:    u64,
}

impl Config {
    pub fn load() -> anyhow::Result<Self> {
        let cfg = config::Config::builder()
            .add_source(config::File::with_name("config/default").required(false))
            .add_source(config::File::with_name("config/production").required(false))
            .add_source(config::Environment::with_prefix("PREDMARKET").separator("__"))
            .build()?;

        Ok(cfg.try_deserialize()?)
    }

    pub fn engine_submit_timeout(&self) -> Duration {
        Duration::from_millis(self.engine.submit_timeout_ms)
    }

    pub fn engine_cancel_timeout(&self) -> Duration {
        Duration::from_millis(self.engine.cancel_timeout_ms)
    }

    pub fn db_connect_timeout(&self) -> Duration {
        Duration::from_secs(self.database.connect_timeout)
    }

    pub fn db_idle_timeout(&self) -> Duration {
        Duration::from_secs(self.database.idle_timeout)
    }

    pub fn db_max_lifetime(&self) -> Duration {
        Duration::from_secs(self.database.max_lifetime)
    }

    pub fn nats_connect_timeout(&self) -> Duration {
        Duration::from_secs(self.nats.connect_timeout)
    }

    pub fn server_addr(&self) -> String {
        format!("{}:{}", self.server.host, self.server.port)
    }
}
