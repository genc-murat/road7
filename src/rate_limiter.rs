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
    Quota {
        quota: u64,
        period_seconds: u64,
        header_key: Option<String>,
    },
    Dynamic {
        initial_rate: u64,
        max_rate: u64,
        min_rate: u64,
        adjust_factor: f64,
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
    Quota(RwLock<QuotaState>, String),
    Dynamic(RwLock<DynamicState>, String),
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

#[derive(Debug)]
struct QuotaState {
    remaining_quota: u64,
    reset_time: Instant,
    quota: u64,
    period_seconds: u64,
}

#[derive(Debug)]
struct DynamicState {
    current_rate: u64,
    last_adjusted: Instant,
    adjust_factor: f64,
    window_seconds: u64,
    min_rate: u64,
    max_rate: u64,
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
            RateLimiterConfig::Quota {
                quota,
                period_seconds,
                header_key: _,
            } => {
                let quota_key = format!("Quota{}", key_suffix);
                RateLimiter::Quota(
                    RwLock::new(QuotaState {
                        remaining_quota: *quota,
                        reset_time: Instant::now() + Duration::from_secs(*period_seconds),
                        quota: *quota,
                        period_seconds: *period_seconds,
                    }),
                    quota_key,
                )
            }
            RateLimiterConfig::Dynamic {
                initial_rate,
                max_rate,
                min_rate,
                adjust_factor,
                window_seconds,
                header_key: _,
            } => {
                let dynamic_key = format!("Dynamic{}", key_suffix);
                RateLimiter::Dynamic(
                    RwLock::new(DynamicState {
                        current_rate: *initial_rate,
                        last_adjusted: Instant::now(),
                        adjust_factor: *adjust_factor,
                        window_seconds: *window_seconds,
                        min_rate: *min_rate,
                        max_rate: *max_rate,
                    }),
                    dynamic_key,
                )
            }
        }
    }

    pub async fn acquire(&self) -> bool {
        match self {
            RateLimiter::TokenBucket(semaphore, refill_rate, _) => {
                if semaphore.try_acquire().is_ok() {
                    let semaphore = semaphore.clone();
                    let refill_rate = *refill_rate as f64; // Convert refill_rate to f64
            
                    tokio::spawn(async move {
                        let delay = Duration::from_secs_f64(1.0 / refill_rate);
                        loop {
                            sleep(delay).await;
                            semaphore.add_permits(1);
                        }
                    });
            
                    true
                } else {
                    false
                }
            }
            
            
            RateLimiter::LeakyBucket(state, _) => {
                let now = Instant::now();
                let (tokens, bucket_size, leak_rate, last_refill) = {
                    let state = state.read().await;
                    (state.tokens, state.bucket_size, state.leak_rate, state.last_refill)
                };
                
                let elapsed = now.duration_since(last_refill).as_secs();
                let leaked_tokens = leak_rate.saturating_mul(elapsed);
                let new_tokens = tokens.saturating_add(leaked_tokens).min(bucket_size);
                
                let should_allow = if new_tokens > 0 {
                    new_tokens - 1
                } else {
                    0
                };
                
                {
                    let mut state = state.write().await;
                    state.tokens = should_allow;
                    state.last_refill = now;
                }
            
                should_allow > 0
            }
            
            RateLimiter::FixedWindow(state, _) => {
                let now = Instant::now();
                let (mut current_requests, window_start, window_seconds, rate) = {
                    let state = state.read().await;
                    (state.current_requests, state.window_start, state.window_seconds, state.rate)
                };
            
                let elapsed = now.duration_since(window_start).as_secs();
                if elapsed >= window_seconds {
                    current_requests = 0;
                }
            
                let should_allow = if current_requests < rate {
                    current_requests + 1
                } else {
                    current_requests
                };
            
                {
                    let mut state = state.write().await;
                    if elapsed >= window_seconds {
                        state.window_start = now;
                    }
                    state.current_requests = should_allow;
                }
            
                should_allow <= rate
            }
            
            RateLimiter::SlidingLog(state, _) => {
                let now = Instant::now();
            
                // Acquire a read lock to access window_seconds and rate
                let (window_seconds, rate) = {
                    let state = state.read().await;
                    (state.window_seconds, state.rate)
                };
            
                let window_start = now - Duration::from_secs(window_seconds);
            
                // Retain only the timestamps within the window
                let mut requests_within_window = {
                    let state = state.read().await;
                    state.requests.iter().filter(|&&t| t >= window_start).cloned().collect::<Vec<_>>()
                };
            
                let allow_request = if requests_within_window.len() < rate as usize {
                    requests_within_window.push(now);
                    true
                } else {
                    false
                };
            
                // Update the state with the new requests log
                if allow_request {
                    let mut state = state.write().await;
                    state.requests = requests_within_window;
                }
            
                allow_request
            }
            
            RateLimiter::SlidingWindow(state, _) => {
                let now = Instant::now();
            
                let (mut current_requests, window_start, window_seconds, rate) = {
                    let state = state.read().await;
                    (state.current_requests, state.window_start, state.window_seconds, state.rate)
                };
            
                let window_end = window_start + Duration::from_secs(window_seconds);
            
                let should_allow = if now >= window_end {
                    current_requests = 1;
                    true
                } else if current_requests < rate {
                    current_requests += 1;
                    true
                } else {
                    false
                };
            
                if should_allow {
                    let mut state = state.write().await;
                    if now >= window_end {
                        state.window_start = now;
                    }
                    state.current_requests = current_requests;
                }
            
                should_allow
            }
            
            RateLimiter::Quota(state, _) => {
                let now = Instant::now();
                let (mut remaining_quota, reset_time, period_seconds, quota) = {
                    let state = state.read().await;
                    (state.remaining_quota, state.reset_time, state.period_seconds, state.quota)
                };
            
                let mut should_reset = false;
            
                if now >= reset_time {
                    remaining_quota = quota;
                    should_reset = true;
                }
            
                let allow_request = if remaining_quota > 0 {
                    remaining_quota -= 1;
                    true
                } else {
                    false
                };
            
                if allow_request || should_reset {
                    let mut state = state.write().await;
                    if should_reset {
                        state.reset_time = now + Duration::from_secs(period_seconds);
                    }
                    state.remaining_quota = remaining_quota;
                }
            
                allow_request
            }
            
            RateLimiter::Dynamic(state, _) => {
                let now = Instant::now();
            
                let (mut current_rate, last_adjusted, window_seconds, adjust_factor, min_rate, max_rate) = {
                    let state = state.read().await;
                    (
                        state.current_rate,
                        state.last_adjusted,
                        state.window_seconds,
                        state.adjust_factor,
                        state.min_rate,
                        state.max_rate,
                    )
                };
            
                let mut should_adjust = false;
            
                if now.duration_since(last_adjusted).as_secs() >= window_seconds {
                    current_rate = (current_rate as f64 * adjust_factor)
                        .clamp(min_rate as f64, max_rate as f64) as u64;
                    should_adjust = true;
                }
            
                let allow_request = if current_rate > 0 {
                    current_rate -= 1;
                    true
                } else {
                    false
                };
            
                if allow_request || should_adjust {
                    let mut state = state.write().await;
                    if should_adjust {
                        state.last_adjusted = now;
                    }
                    state.current_rate = current_rate;
                }
            
                allow_request
            }
            
        }
    }
}