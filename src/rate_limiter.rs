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
    last_acquire: RwLock<Instant>,
    shutdown: tokio::sync::Notify,
}

#[derive(Serialize)]
pub struct RateLimiterStatus {
    capacity: usize,
    burst_capacity: usize,
    tokens: usize,
    max_rate: usize,
    period: Duration,
    last_refill: String,
    last_acquire: String,
}

impl RateLimiter {
    pub fn new(config: RateLimiterConfig) -> Arc<Self> {
        if config.capacity == 0 || config.max_rate == 0 {
            panic!("Invalid RateLimiterConfig: capacity and max_rate must be greater than 0");
        }

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
        let interval_duration = limiter.config.period / (limiter.config.max_rate as u32);
        tokio::spawn(async move {
            let mut interval = time::interval(interval_duration);
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let mut tokens = limiter.tokens.write().await;
                        *tokens = (*tokens + 1).min(limiter.config.capacity + limiter.config.burst_capacity);
                        *limiter.last_refill.write().await = Instant::now();
                        //info!("Tokens refilled. Current tokens: {}", *tokens);
                    },
                    _ = limiter.shutdown.notified() => {
                        info!("Rate limiter shutdown initiated.");
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
            error!("Rate limit exceeded.");
            Err(RateLimitError::Exhausted)
        }
    }

    pub async fn shutdown(&self) {
        self.shutdown.notify_waiters();
        info!("Rate limiter shutdown completed.");
    }
}

#[derive(Debug,PartialEq)]
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

        if let Some(limiter) = limiters.get(target) {
            return Some(limiter.clone());
        }

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
        if let Some(existing_limiter) = limiters.get_mut(&target) {
            *existing_limiter = RateLimiter::new(config);
        } else {
            limiters.insert(target.clone(), RateLimiter::new(config));
        }
        info!("Rate limiter added for target: {}", target);
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
        info!("All rate limiters have been shut down.");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::time::sleep;

    #[tokio::test]
    async fn test_rate_limiter_creation() {
        let config = RateLimiterConfig {
            capacity: 10,
            burst_capacity: 5,
            max_rate: 2,
            period: Duration::from_secs(1),
        };
        let limiter = RateLimiter::new(config.clone());
        let status = limiter.status().await;

        assert_eq!(status.capacity, config.capacity);
        assert_eq!(status.burst_capacity, config.burst_capacity);
        assert_eq!(status.max_rate, config.max_rate);
        assert_eq!(status.tokens, config.capacity + config.burst_capacity);
    }

    #[tokio::test]
    async fn test_rate_limiter_acquire() {
        let config = RateLimiterConfig {
            capacity: 1,
            burst_capacity: 0,
            max_rate: 1,
            period: Duration::from_secs(1),
        };
        let limiter = RateLimiter::new(config);
        
        // First acquire should succeed
        assert_eq!(limiter.acquire().await, Ok(()));
        
        // Second acquire should fail
        assert_eq!(limiter.acquire().await, Err(RateLimitError::Exhausted));

        // Wait for refill
        sleep(Duration::from_secs(1)).await;
        
        // Acquire after refill should succeed
        assert_eq!(limiter.acquire().await, Ok(()));
    }

    #[tokio::test]
    async fn test_rate_limiter_status() {
        let config = RateLimiterConfig {
            capacity: 5,
            burst_capacity: 5,
            max_rate: 1,
            period: Duration::from_secs(1),
        };
        let limiter = RateLimiter::new(config);
        
        let status = limiter.status().await;
        assert_eq!(status.tokens, 10);
        
        limiter.acquire().await.unwrap();
        let status = limiter.status().await;
        assert_eq!(status.tokens, 9);
    }

    #[tokio::test]
    async fn test_rate_limiter_manager() {
        let manager = RateLimiterManager::new();
        
        let config = RateLimiterConfig {
            capacity: 5,
            burst_capacity: 5,
            max_rate: 1,
            period: Duration::from_secs(1),
        };
        
        manager.add_limiter("api/test".to_string(), config.clone()).await;
        
        let limiter = manager.get_limiter("api/test").await;
        assert!(limiter.is_some());
        
        let limiter = limiter.unwrap();
        let status = limiter.status().await;
        assert_eq!(status.capacity, config.capacity);
        
        let all_statuses = manager.get_all_statuses().await;
        assert_eq!(all_statuses.len(), 1);
    }

    #[tokio::test]
    async fn test_rate_limiter_manager_prefix() {
        let manager = RateLimiterManager::new();

        let config1 = RateLimiterConfig {
            capacity: 5,
            burst_capacity: 5,
            max_rate: 1,
            period: Duration::from_secs(1),
        };

        let config2 = RateLimiterConfig {
            capacity: 10,
            burst_capacity: 5,
            max_rate: 2,
            period: Duration::from_secs(1),
        };

        manager.add_limiter("api/v1/".to_string(), config1.clone()).await;
        manager.add_limiter("api/v1/user/".to_string(), config2.clone()).await;

        let limiter = manager.get_limiter("api/v1/user/123").await;
        assert!(limiter.is_some());

        let limiter = limiter.unwrap();
        let status = limiter.status().await;
        assert_eq!(status.capacity, config2.capacity);

        let all_statuses = manager.get_all_statuses().await;
        assert_eq!(all_statuses.len(), 2);
    }
}
