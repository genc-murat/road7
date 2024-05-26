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
    last_acquire: RwLock<Instant>, // Son başarılı alma zamanını izlemek için
    shutdown: tokio::sync::Notify,
}

#[derive(Serialize)]
pub struct RateLimiterStatus {
    capacity: usize,
    burst_capacity: usize,
    tokens: usize,
    max_rate: usize,
    period: Duration,
    last_refill: String,  // Daha iyi okunabilirlik için String'e dönüştürün
    last_acquire: String, // Daha iyi okunabilirlik için String'e dönüştürün
}

impl RateLimiter {
    pub fn new(config: RateLimiterConfig) -> Arc<Self> {
        let limiter = Arc::new(Self {
            tokens: RwLock::new(config.capacity + config.burst_capacity),
            last_refill: RwLock::new(Instant::now()),
            last_acquire: RwLock::new(Instant::now()),
            config,
            shutdown: tokio::sync::Notify::new(),
        });
        limiter.start_refill_task();
        limiter
    }

    fn start_refill_task(self: &Arc<Self>) {
        let limiter = self.clone();
        tokio::spawn(async move {
            let mut interval = time::interval(limiter.config.period);
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let mut tokens = limiter.tokens.write().await;
                        *tokens = limiter.config.capacity + limiter.config.burst_capacity;
                        *limiter.last_refill.write().await = Instant::now();
                        info!("Tokens refilled to capacity: {}", limiter.config.capacity + limiter.config.burst_capacity);
                    },
                    _ = limiter.shutdown.notified() => {
                        info!("Rate limiter shutdown.");
                        break;
                    }
                }
            }
        });
    }

    pub async fn status(&self) -> RateLimiterStatus {
        let tokens = *self.tokens.read().await;
        let last_refill = *self.last_refill.read().await;
        let last_acquire = *self.last_acquire.read().await;

        RateLimiterStatus {
            capacity: self.config.capacity,
            burst_capacity: self.config.burst_capacity,
            tokens,
            max_rate: self.config.max_rate,
            period: self.config.period,
            last_refill: format!("{:?}", last_refill),
            last_acquire: format!("{:?}", last_acquire),
        }
    }

    pub async fn acquire(&self) -> Result<(), RateLimitError> {
        let mut tokens = self.tokens.write().await;
        if *tokens > 0 {
            *tokens -= 1;
            *self.last_acquire.write().await = Instant::now();
            info!("Token acquired, remaining tokens: {}", *tokens);
            Ok(())
        } else {
            Err(RateLimitError::Exhausted)
        }
    }

    pub async fn shutdown(&self) {
        self.shutdown.notify_waiters();
    }
}

#[derive(Debug)]
pub enum RateLimitError {
    Exhausted,
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

        // Tam eşleşmeyi deneyin
        if let Some(limiter) = limiters.get(target) {
            return Some(limiter.clone());
        }

        // Önek eşleşmesini deneyin
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
        // Eşleşen bir hedef varsa mevcut limitleyiciyi güncelleyin
        if let Some(existing_limiter) = limiters.get_mut(&target) {
            // Mevcut limitleyiciyi yeni yapılandırmayla güncelleyin
            *existing_limiter = RateLimiter::new(config);
        } else {
            limiters.insert(target, RateLimiter::new(config));
        }
    }

    pub async fn get_all_statuses(&self) -> HashMap<String, RateLimiterStatus> {
        let limiters = self.limiters.read().await;
        let mut statuses = HashMap::new();
        for (path, limiter) in limiters.iter() {
            statuses.insert(path.clone(), limiter.status().await);
        }
        statuses
    }

    pub async fn shutdown(&self) {
        let limiters = self.limiters.read().await;
        for limiter in limiters.values() {
            limiter.shutdown().await;
        }
    }
}