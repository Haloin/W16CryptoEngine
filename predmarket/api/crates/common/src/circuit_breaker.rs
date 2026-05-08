use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: usize,
    pub reset_timeout: Duration,
    pub failure_window: Duration,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            reset_timeout: Duration::from_secs(30),
            failure_window: Duration::from_secs(60),
        }
    }
}

pub struct CircuitBreaker {
    state: RwLock<CircuitState>,
    config: CircuitBreakerConfig,
    failures: AtomicUsize,
    last_failure_time: AtomicU64,
    last_state_change: AtomicU64,
    name: String,
}

impl CircuitBreaker {
    pub fn new(name: impl Into<String>, config: CircuitBreakerConfig) -> Self {
        Self {
            state: RwLock::new(CircuitState::Closed),
            config,
            failures: AtomicUsize::new(0),
            last_failure_time: AtomicU64::new(0),
            last_state_change: AtomicU64::new(0),
            name: name.into(),
        }
    }

    pub async fn allow_call(&self) -> bool {
        let mut state = self.state.write().await;
        let now = Instant::now().elapsed().as_secs();

        match *state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                let last_change = self.last_state_change.load(Ordering::Relaxed);
                if now.saturating_sub(last_change) >= self.config.reset_timeout.as_secs() {
                    tracing::info!(
                        circuit_breaker = %self.name,
                        "Circuit breaker entering half-open state"
                    );
                    *state = CircuitState::HalfOpen;
                    self.last_state_change.store(now, Ordering::Relaxed);
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => true,
        }
    }

    pub async fn record_success(&self) {
        let mut state = self.state.write().await;

        if *state == CircuitState::HalfOpen {
            tracing::info!(
                circuit_breaker = %self.name,
                "Circuit breaker closing - service recovered"
            );
            *state = CircuitState::Closed;
            self.failures.store(0, Ordering::Relaxed);
            self.last_state_change.store(
                Instant::now().elapsed().as_secs(),
                Ordering::Relaxed,
            );
        }
    }

    pub async fn record_failure(&self) {
        let now = Instant::now().elapsed().as_secs();
        self.last_failure_time.store(now, Ordering::Relaxed);

        let failures = self.failures.fetch_add(1, Ordering::Relaxed) + 1;

        let mut state = self.state.write().await;

        if *state == CircuitState::HalfOpen {
            tracing::error!(
                circuit_breaker = %self.name,
                "Circuit breaker opening - service still failing"
            );
            *state = CircuitState::Open;
            self.last_state_change.store(now, Ordering::Relaxed);
        } else if *state == CircuitState::Closed && failures >= self.config.failure_threshold {
            tracing::error!(
                circuit_breaker = %self.name,
                failures = failures,
                threshold = self.config.failure_threshold,
                "Circuit breaker opening - failure threshold reached"
            );
            *state = CircuitState::Open;
            self.last_state_change.store(now, Ordering::Relaxed);
        }
    }

    pub async fn state(&self) -> CircuitState {
        *self.state.read().await
    }

    pub async fn reset(&self) {
        let mut state = self.state.write().await;
        *state = CircuitState::Closed;
        self.failures.store(0, Ordering::Relaxed);
        self.last_state_change.store(
            Instant::now().elapsed().as_secs(),
            Ordering::Relaxed,
        );
        tracing::info!(circuit_breaker = %self.name, "Circuit breaker manually reset");
    }
}

pub struct CircuitBreakerRegistry {
    breakers: Arc<RwLock<std::collections::HashMap<String, Arc<CircuitBreaker>>>>,
}

impl CircuitBreakerRegistry {
    pub fn new() -> Self {
        Self {
            breakers: Arc::new(RwLock::new(std::collections::HashMap::new())),
        }
    }

    pub async fn get_or_create(
        &self,
        name: impl Into<String>,
        config: CircuitBreakerConfig,
    ) -> Arc<CircuitBreaker> {
        let name = name.into();
        let breakers: tokio::sync::RwLockReadGuard<'_, std::collections::HashMap<String, Arc<CircuitBreaker>>> = self.breakers.read().await;
        
        if let Some(breaker) = breakers.get(&name) {
            return Arc::clone(breaker);
        }
        drop(breakers);

        let mut breakers: tokio::sync::RwLockWriteGuard<'_, std::collections::HashMap<String, Arc<CircuitBreaker>>> = self.breakers.write().await;
        let breaker = Arc::new(CircuitBreaker::new(&name, config));
        breakers.insert(name, Arc::clone(&breaker));
        breaker
    }

    pub async fn get(&self, name: &str) -> Option<Arc<CircuitBreaker>> {
        let breakers: tokio::sync::RwLockReadGuard<'_, std::collections::HashMap<String, Arc<CircuitBreaker>>> = self.breakers.read().await;
        breakers.get(name).map(Arc::clone)
    }
}

impl Default for CircuitBreakerRegistry {
    fn default() -> Self {
        Self::new()
    }
}
