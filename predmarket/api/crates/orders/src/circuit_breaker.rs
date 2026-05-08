use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

pub struct CircuitBreaker {
    failure_threshold: u32,
    recovery_duration: Duration,
    failure_count: AtomicU32,
    last_failure: RwLock<Option<Instant>>,
    state: RwLock<CircuitState>,
    half_open_attempts: AtomicU32,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, recovery_secs: u64) -> Self {
        Self {
            failure_threshold,
            recovery_duration: Duration::from_secs(recovery_secs),
            failure_count: AtomicU32::new(0),
            last_failure: RwLock::new(None),
            state: RwLock::new(CircuitState::Closed),
            half_open_attempts: AtomicU32::new(0),
        }
    }

    pub async fn call<F, Fut, T>(&self, f: F) -> Result<T, CircuitError>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = Result<T, anyhow::Error>>,
    {
        let state = *self.state.read().await;

        match state {
            CircuitState::Open => {
                let should_attempt_recovery = {
                    let last = *self.last_failure.read().await;
                    last.map(|t| t.elapsed() >= self.recovery_duration).unwrap_or(true)
                };

                if should_attempt_recovery {
                    let mut s = self.state.write().await;
                    *s = CircuitState::HalfOpen;
                    self.half_open_attempts.store(0, Ordering::SeqCst);
                } else {
                    return Err(CircuitError::Open);
                }
            }
            CircuitState::HalfOpen => {
                let attempts = self.half_open_attempts.fetch_add(1, Ordering::SeqCst);
                if attempts >= 1 {
                    return Err(CircuitError::Open);
                }
            }
            CircuitState::Closed => {}
        }

        let result = f().await;

        match result {
            Ok(v) => {
                if *self.state.read().await == CircuitState::HalfOpen {
                    let mut s = self.state.write().await;
                    *s = CircuitState::Closed;
                    self.failure_count.store(0, Ordering::SeqCst);
                }
                Ok(v)
            }
            Err(_) => {
                self.record_failure().await;
                Err(CircuitError::Underlying)
            }
        }
    }

    async fn record_failure(&self) {
        let count = self.failure_count.fetch_add(1, Ordering::SeqCst) + 1;
        *self.last_failure.write().await = Some(Instant::now());

        if count >= self.failure_threshold {
            let mut s = self.state.write().await;
            *s = CircuitState::Open;
        }
    }

    pub async fn state(&self) -> CircuitState {
        *self.state.read().await
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitError {
    Open,
    Underlying,
}

impl std::fmt::Display for CircuitError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitError::Open => write!(f, "circuit breaker open"),
            CircuitError::Underlying => write!(f, "underlying operation failed"),
        }
    }
}

impl std::error::Error for CircuitError {}

pub type CircuitBreakerRef = Arc<CircuitBreaker>;
