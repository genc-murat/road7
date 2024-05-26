use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::time::{self, Duration, Instant};
use std::collections::HashMap;
use tracing::{info, error};

#[derive(Clone)]
pub struct RateLimiterConfig {
    pub capacity: usize,
    pub max_rate: usize,
    pub period: Duration,
}

pub struct RateLimiter {
    config: RateLimiterConfig,
    tokens: RwLock<usize>,
    last_refill: RwLock<Instant>,
}

impl RateLimiter {
    pub fn new(config: RateLimiterConfig) -> Arc<Self> {
        Arc::new(Self {
            tokens: RwLock::new(config.capacity),
            last_refill: RwLock::new(Instant::now()),
            config,
        })
    }

    pub async fn acquire(&self) -> bool {
        let mut tokens = self.tokens.write().await;
        let mut last_refill = self.last_refill.write().await;

        let elapsed = last_refill.elapsed();
        if elapsed >= self.config.period {
            *tokens = self.config.capacity;
            *last_refill = Instant::now();
            info!("Refilled tokens to capacity: {}", self.config.capacity);
        }

        if *tokens > 0 {
            *tokens -= 1;
            info!("Token acquired, remaining tokens: {}", *tokens);
            true
        } else {
            error!("Rate limit exceeded, request denied");
            false
        }
    }
}

pub struct RateLimiterManager {
    limiters: RwLock<HashMap<String, Arc<RateLimiter>>>,
}

impl RateLimiterManager {
    pub fn new() -> Self {
        Self {
            limiters: RwLock::new(HashMap::new()),
        }
    }

    pub async fn get_limiter(&self, target: &str) -> Option<Arc<RateLimiter>> {
        let limiters = self.limiters.read().await;
        info!("Checking rate limiter for target: {}", target);

        // Try exact match
        if let Some(limiter) = limiters.get(target) {
            return Some(limiter.clone());
        }

        // Try prefix match
        for (path, limiter) in limiters.iter() {
            if target.starts_with(path) {
                info!("Rate limiter found for path prefix: {}", path);
                return Some(limiter.clone());
            }
        }

        None
    }

    pub async fn add_limiter(&self, target: String, config: RateLimiterConfig) {
        let mut limiters = self.limiters.write().await;
        limiters.insert(target, RateLimiter::new(config));
    }
}
