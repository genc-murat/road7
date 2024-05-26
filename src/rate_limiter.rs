use std::sync::Arc;
use serde::Serialize;
use tokio::sync::RwLock;
use tokio::time::{self, Duration, Instant};
use std::collections::HashMap;
use tracing::{info, error};

#[derive(Clone)]
pub struct RateLimiterConfig {
    pub capacity: usize,
    pub burst_capacity: usize,
    pub max_rate: usize,
    pub period: Duration,
}

pub struct RateLimiter {
    config: RateLimiterConfig,
    tokens: RwLock<usize>,
    last_refill: RwLock<Instant>,
}

#[derive(Serialize)]
pub struct RateLimiterStatus {
    capacity: usize,
    burst_capacity: usize,
    tokens: usize,
    max_rate: usize,
    period: Duration,
    last_refill: String,  // Convert to String for better readability
}

impl RateLimiter {
    pub fn new(config: RateLimiterConfig) -> Arc<Self> {
        Arc::new(Self {
            tokens: RwLock::new(config.capacity + config.burst_capacity),
            last_refill: RwLock::new(Instant::now()),
            config,
        })
    }

    pub async fn status(&self) -> RateLimiterStatus {
        let tokens = *self.tokens.read().await;
        let last_refill = *self.last_refill.read().await;

        RateLimiterStatus {
            capacity: self.config.capacity,
            burst_capacity: self.config.burst_capacity,
            tokens,
            max_rate: self.config.max_rate,
            period: self.config.period,
            last_refill: format!("{:?}", last_refill),
        }
    }

    pub async fn acquire(&self) -> bool {
        let mut tokens = self.tokens.write().await;
        let mut last_refill = self.last_refill.write().await;

        let elapsed = last_refill.elapsed();
        let leak_rate = self.config.max_rate as f64 / self.config.period.as_secs_f64();
        let leaked_tokens = (elapsed.as_secs_f64() * leak_rate).min(*tokens as f64) as usize;

        *tokens = tokens.saturating_sub(leaked_tokens).min(self.config.capacity + self.config.burst_capacity);
        if elapsed >= self.config.period {
            *tokens = self.config.capacity + self.config.burst_capacity;
            *last_refill = Instant::now();
            info!("Refilled tokens to capacity: {}", self.config.capacity + self.config.burst_capacity);
        } else {
            *last_refill += Duration::from_secs_f64(leaked_tokens as f64 / leak_rate);
            info!("Leaked tokens: {}, remaining tokens: {}", leaked_tokens, *tokens);
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

    pub async fn get_all_statuses(&self) -> HashMap<String, RateLimiterStatus> { // Add this method
        let limiters = self.limiters.read().await;
        let mut statuses = HashMap::new();
        for (path, limiter) in limiters.iter() {
            statuses.insert(path.clone(), limiter.status().await);
        }
        statuses
    }
}

