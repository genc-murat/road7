use std::sync::Arc;
use serde::{Deserialize, Serialize};
use tokio::sync::{RwLock, Semaphore};
use std::time::Instant;
use tokio::time::{sleep, Duration};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum RateLimiterConfig {
    TokenBucket {
        refill_rate: u64,
        burst_capacity: u64,
        header_key: Option<String>,
    },
    LeakyBucket {
        leak_rate: u64,
        bucket_size: u64,
        header_key: Option<String>,
    },
    FixedWindow {
        rate: u64,
        window_seconds: u64,
        header_key: Option<String>,
    },
    SlidingLog {
        rate: u64,
        window_seconds: u64,
        header_key: Option<String>,
    },
    SlidingWindow {
        rate: u64,
        window_seconds: u64,
        header_key: Option<String>,
    },
}

impl Default for RateLimiterConfig {
    fn default() -> Self {
        RateLimiterConfig::TokenBucket {
            refill_rate: 10,
            burst_capacity: 20,
            header_key: None,
        }
    }
}

#[derive(Debug)]
pub enum RateLimiter {
    TokenBucket(Arc<Semaphore>, u64, String),
    LeakyBucket(RwLock<LeakyBucketState>, String),
    FixedWindow(RwLock<FixedWindowState>, String),
    SlidingLog(RwLock<SlidingLogState>, String),
    SlidingWindow(RwLock<SlidingWindowState>, String),
}

#[derive(Debug)]
struct LeakyBucketState {
    tokens: u64,
    last_refill: Instant,
    leak_rate: u64,
    bucket_size: u64,
}

#[derive(Debug)]
struct FixedWindowState {
    current_requests: u64,
    window_start: Instant,
    rate: u64,
    window_seconds: u64,
}

#[derive(Debug)]
struct SlidingLogState {
    requests: Vec<Instant>,
    rate: u64,
    window_seconds: u64,
}

#[derive(Debug)]
struct SlidingWindowState {
    current_requests: u64,
    window_start: Instant,
    rate: u64,
    window_seconds: u64,
}

impl RateLimiter {
    pub fn new(config: &RateLimiterConfig, header_value: Option<&str>) -> Self {
        let key_suffix = header_value.unwrap_or_default();
        match config {
            RateLimiterConfig::TokenBucket {
                refill_rate,
                burst_capacity,
                header_key: _,
            } => {
                let semaphore_key = format!("TokenBucket{}", key_suffix);
                RateLimiter::TokenBucket(
                    Arc::new(Semaphore::new(*burst_capacity as usize)),
                    *refill_rate,
                    semaphore_key,
                )
            }
            RateLimiterConfig::LeakyBucket {
                leak_rate,
                bucket_size,
                header_key: _,
            } => {
                let bucket_key = format!("LeakyBucket{}", key_suffix);
                RateLimiter::LeakyBucket(
                    RwLock::new(LeakyBucketState {
                        tokens: *bucket_size,
                        last_refill: Instant::now(),
                        leak_rate: *leak_rate,
                        bucket_size: *bucket_size,
                    }),
                    bucket_key,
                )
            }
            RateLimiterConfig::FixedWindow {
                rate,
                window_seconds,
                header_key: _,
            } => {
                let window_key = format!("FixedWindow{}", key_suffix);
                RateLimiter::FixedWindow(
                    RwLock::new(FixedWindowState {
                        current_requests: 0,
                        window_start: Instant::now(),
                        rate: *rate,
                        window_seconds: *window_seconds,
                    }),
                    window_key,
                )
            }
            RateLimiterConfig::SlidingLog {
                rate,
                window_seconds,
                header_key: _,
            } => {
                let log_key = format!("SlidingLog{}", key_suffix);
                RateLimiter::SlidingLog(
                    RwLock::new(SlidingLogState {
                        requests: Vec::new(),
                        rate: *rate,
                        window_seconds: *window_seconds,
                    }),
                    log_key,
                )
            }
            RateLimiterConfig::SlidingWindow {
                rate,
                window_seconds,
                header_key: _,
            } => {
                let window_key = format!("SlidingWindow{}", key_suffix);
                RateLimiter::SlidingWindow(
                    RwLock::new(SlidingWindowState {
                        current_requests: 0,
                        window_start: Instant::now(),
                        rate: *rate,
                        window_seconds: *window_seconds,
                    }),
                    window_key,
                )
            }
        }
    }

    pub async fn acquire(&self) -> bool {
        match self {
            RateLimiter::TokenBucket(semaphore, refill_rate, _) => {
                if let Ok(permit) = semaphore.clone().try_acquire_owned() {
                    let permit = Arc::new(permit);
                    let refill_rate = *refill_rate;
                    tokio::spawn(async move {
                        let delay = Duration::from_secs(1) / refill_rate as u32;
                        loop {
                            sleep(delay).await;
                            drop(permit.clone());
                        }
                    });
                    true
                } else {
                    false
                }
            }
            RateLimiter::LeakyBucket(state, _) => {
                let mut state = state.write().await;
                let now = Instant::now();
                let elapsed = now.duration_since(state.last_refill).as_secs_f64();
                let leaked_tokens = (state.leak_rate as f64 * elapsed).floor() as u64;
                state.tokens =
                    state.tokens.saturating_add(leaked_tokens).min(state.bucket_size);
                state.last_refill = now;
                if state.tokens > 0 {
                    state.tokens -= 1;
                    true
                } else {
                    false
                }
            }
            RateLimiter::FixedWindow(state, _) => {
                let mut state = state.write().await;
                let now = Instant::now();
                if now.duration_since(state.window_start).as_secs() >= state.window_seconds {

                    state.current_requests = 0;
                    state.window_start = now;
                }
                if state.current_requests < state.rate {
                    state.current_requests += 1;
                    true
                } else {
                    false
                }
            }
            RateLimiter::SlidingLog(state, _) => {
                let mut state = state.write().await;
                let now = Instant::now();
                let window_start = now - Duration::from_secs(state.window_seconds);

                state.requests.retain(|&t| t >= window_start);
                if state.requests.len() < state.rate as usize {
                    state.requests.push(now);
                    true
                } else {
                    false
                }
            }
            RateLimiter::SlidingWindow(state, _) => {
                let mut state = state.write().await;
                let now = Instant::now();
                let window_end =
                    state.window_start + Duration::from_secs(state.window_seconds);
                if now >= window_end {
                    state.window_start = now;
                    state.current_requests = 1;
                    true
                } else if state.current_requests < state.rate {
                    state.current_requests += 1;
                    true
                } else {
                    false
                }
            }
        }
    }
}
