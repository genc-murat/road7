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
    TokenBucket(Arc<Semaphore>, Arc<SemaphoreRefill>),
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

#[derive(Debug, Clone)]
struct SemaphoreRefill {
    refill_rate: u64,
    semaphore: Arc<Semaphore>,
}

impl SemaphoreRefill {
    fn new(refill_rate: u64, semaphore: Arc<Semaphore>) -> Self {
        let refill = Self { refill_rate, semaphore: semaphore.clone() };
        let refill_clone = refill.clone();
        tokio::spawn(async move {
            let delay = Duration::from_secs_f64(1.0 / refill_rate as f64);
            loop {
                sleep(delay).await;
                semaphore.add_permits(1);
            }
        });
        refill
    }
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
                let semaphore = Arc::new(Semaphore::new(*burst_capacity as usize));
                let semaphore_refill = Arc::new(SemaphoreRefill::new(*refill_rate, semaphore.clone()));
                RateLimiter::TokenBucket(semaphore, semaphore_refill)
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
            RateLimiter::TokenBucket(semaphore, _) => {
                semaphore.try_acquire().is_ok()
            }
            RateLimiter::LeakyBucket(state, _) => {
                let now = Instant::now();
                let mut state = state.write().await;
                let elapsed = now.duration_since(state.last_refill).as_secs();
                let leaked_tokens = state.leak_rate.saturating_mul(elapsed);
                state.tokens = state.tokens.saturating_add(leaked_tokens).min(state.bucket_size);
                state.last_refill = now;

                if state.tokens > 0 {
                    state.tokens -= 1;
                    true
                } else {
                    false
                }
            }
            RateLimiter::FixedWindow(state, _) => {
                let now = Instant::now();
                let mut state = state.write().await;
                let elapsed = now.duration_since(state.window_start).as_secs();
                if elapsed >= state.window_seconds {
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
                let now = Instant::now();
                let mut state = state.write().await;
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
                let now = Instant::now();
                let mut state = state.write().await;
                let window_end = state.window_start + Duration::from_secs(state.window_seconds);

                if now >= window_end {
                    state.current_requests = 1;
                    state.window_start = now;
                    true
                } else if state.current_requests < state.rate {
                    state.current_requests += 1;
                    true
                } else {
                    false
                }
            }
            RateLimiter::Quota(state, _) => {
                let now = Instant::now();
                let mut state = state.write().await;
                if now >= state.reset_time {
                    state.remaining_quota = state.quota;
                    state.reset_time = now + Duration::from_secs(state.period_seconds);
                }

                if state.remaining_quota > 0 {
                    state.remaining_quota -= 1;
                    true
                } else {
                    false
                }
            }
            RateLimiter::Dynamic(state, _) => {
                let now = Instant::now();
                let mut state = state.write().await;
                if now.duration_since(state.last_adjusted).as_secs() >= state.window_seconds {
                    state.current_rate = (state.current_rate as f64 * state.adjust_factor)
                        .clamp(state.min_rate as f64, state.max_rate as f64) as u64;
                    state.last_adjusted = now;
                }

                if state.current_rate > 0 {
                    state.current_rate -= 1;
                    true
                } else {
                    false
                }
            }
        }
    }
}
