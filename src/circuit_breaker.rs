
use std::time::{Duration, Instant};

use tracing::debug;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CircuitBreakerConfig {
    pub max_failures: usize,
    pub reset_timeout_seconds: u64,
    pub half_open_attempts: usize,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        CircuitBreakerConfig {
            max_failures: 3,
            reset_timeout_seconds: 30,
            half_open_attempts: 1,
        }
    }
}

#[derive(Debug)]
pub struct CircuitBreaker {
    state: CircuitState,
    failure_count: usize,
    last_attempt: Instant,
    open_until: Instant,
    reset_timeout: Duration,
    max_failures: usize,
    half_open_attempts: usize,
    half_open_max_attempts: usize,
}

impl CircuitBreaker {
    pub fn new(config: &CircuitBreakerConfig) -> Self {
        let now = Instant::now();
        CircuitBreaker {
            state: CircuitState::Closed,
            failure_count: 0,
            last_attempt: now,
            open_until: now,
            reset_timeout: Duration::from_secs(config.reset_timeout_seconds),
            max_failures: config.max_failures,
            half_open_attempts: 0,
            half_open_max_attempts: config.half_open_attempts,
        }
    }

    pub async fn record_success(&mut self) {
        debug!("Circuit breaker: Success recorded");
        self.failure_count = 0;
        self.state = CircuitState::Closed;
        self.half_open_attempts = 0;
    }

    pub async fn record_failure(&mut self) {
        debug!("Circuit breaker: Failure recorded");
        self.failure_count += 1;
        self.last_attempt = Instant::now();
        if self.failure_count >= self.max_failures {
            self.transition_to_open().await;
        }
    }

    pub async fn can_attempt(&mut self) -> bool {
        let now = Instant::now();
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                if now >= self.open_until {
                    self.transition_to_half_open().await;
                    true
                } else {
                    false
                }
            }
            CircuitState::HalfOpen => {
                if self.half_open_attempts < self.half_open_max_attempts {
                    self.half_open_attempts += 1;
                    true
                } else {
                    false
                }
            }
        }
    }

    pub async fn transition_to_closed(&mut self) {
        debug!("Circuit breaker transitioning to Closed");
        self.state = CircuitState::Closed;
        self.failure_count = 0;
        self.half_open_attempts = 0;
    }

    pub async fn transition_to_open(&mut self) {
        debug!("Circuit breaker transitioning to Open");
        self.state = CircuitState::Open;
        self.open_until = Instant::now() + self.reset_timeout;
        self.half_open_attempts = 0;
    }

    pub async fn transition_to_half_open(&mut self) {
        debug!("Circuit breaker transitioning to Half-Open");
        self.state = CircuitState::HalfOpen;
        self.half_open_attempts = 0;
    }

    pub fn get_state(&self) -> CircuitState {
        self.state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_circuit_breaker_initial_state() {
        let config = CircuitBreakerConfig::default();
        let circuit_breaker = CircuitBreaker::new(&config);

        assert_eq!(circuit_breaker.get_state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_record_success_resets_failure_count_and_state() {
        let config = CircuitBreakerConfig::default();
        let mut circuit_breaker = CircuitBreaker::new(&config);

        circuit_breaker.record_failure().await;
        circuit_breaker.record_failure().await;
        circuit_breaker.record_success().await;

        assert_eq!(circuit_breaker.failure_count, 0);
        assert_eq!(circuit_breaker.get_state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_transition_to_open_after_max_failures() {
        let config = CircuitBreakerConfig::default();
        let mut circuit_breaker = CircuitBreaker::new(&config);

        for _ in 0..config.max_failures {
            circuit_breaker.record_failure().await;
        }

        assert_eq!(circuit_breaker.get_state(), CircuitState::Open);
    }

    #[tokio::test]
    async fn test_transition_to_half_open_after_reset_timeout() {
        let config = CircuitBreakerConfig::default();
        let mut circuit_breaker = CircuitBreaker::new(&config);

        for _ in 0..config.max_failures {
            circuit_breaker.record_failure().await;
        }

        assert_eq!(circuit_breaker.get_state(), CircuitState::Open);

        sleep(Duration::from_secs(config.reset_timeout_seconds)).await;

        let can_attempt = circuit_breaker.can_attempt().await;
        assert!(can_attempt);
        assert_eq!(circuit_breaker.get_state(), CircuitState::HalfOpen);
    }

    #[tokio::test]
    async fn test_transition_back_to_closed_from_half_open_on_success() {
        let config = CircuitBreakerConfig::default();
        let mut circuit_breaker = CircuitBreaker::new(&config);

        for _ in 0..config.max_failures {
            circuit_breaker.record_failure().await;
        }

        sleep(Duration::from_secs(config.reset_timeout_seconds)).await;
        circuit_breaker.can_attempt().await;

        circuit_breaker.record_success().await;

        assert_eq!(circuit_breaker.get_state(), CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_transition_back_to_open_from_half_open_on_failure() {
        let config = CircuitBreakerConfig::default();
        let mut circuit_breaker = CircuitBreaker::new(&config);

        for _ in 0..config.max_failures {
            circuit_breaker.record_failure().await;
        }

        sleep(Duration::from_secs(config.reset_timeout_seconds)).await;
        circuit_breaker.can_attempt().await;

        circuit_breaker.record_failure().await;

        assert_eq!(circuit_breaker.get_state(), CircuitState::Open);
    }
}
