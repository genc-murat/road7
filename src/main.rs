mod retry;

use retry::{RetryStrategy, FixedIntervalBackoffStrategy, ExponentialBackoffStrategy, LinearBackoffStrategy, RandomDelayStrategy, IncrementalBackoffStrategy, FibonacciBackoffStrategy, GeometricBackoffStrategy, HarmonicBackoffStrategy, JitterBackoffStrategy};


use hyper::{Body, Client, Request, Response, Server, StatusCode, Uri};
use hyper::service::{make_service_fn, service_fn};
use hyper::header::{
    HeaderValue, STRICT_TRANSPORT_SECURITY, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS, CONTENT_SECURITY_POLICY, X_XSS_PROTECTION, REFERRER_POLICY, ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, USER_AGENT, HOST, VARY
};
use hyper::client::HttpConnector;
use hyper_tls::HttpsConnector;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Duration, timeout};
use tracing::{debug, error, info, warn};
use tracing_subscriber;
use std::convert::Infallible;
use tokio::signal;
use std::time::Instant;
use std::net::SocketAddr;
use rand::{Rng, thread_rng};
use tracing_appender::non_blocking;
use std::hash::{Hash, Hasher};
use dashmap::DashMap;
use http::header::{HeaderMap, HeaderName};
use std::borrow::Cow;
use regex::Regex;
use ammonia::clean;
use hyper::body::to_bytes;
use uuid::Uuid;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::runtime::Builder;
use thiserror::Error;

const PERMISSIONS_POLICY: HeaderName = HeaderName::from_static("permissions-policy");
const FEATURE_POLICY: HeaderName = HeaderName::from_static("feature-policy");

#[derive(Debug, Deserialize, Serialize)]
struct ProxyConfig {
    server: ServerConfig,
    runtime: RuntimeConfig,
    targets: Vec<Target>,
    retries: RetryConfig,
    #[serde(default)]
    default_circuit_breaker_config: CircuitBreakerConfig,
    #[serde(default = "default_timeout_seconds")]
    default_timeout_seconds: u64,
    #[serde(default)]
    default_rate_limiter_config: Option<RateLimiterConfig>,
    #[serde(default)]
    security_headers_config: Option<SecurityHeadersConfig>,
}

fn default_timeout_seconds() -> u64 {
    30
}

fn default_pool_size() -> usize {
    10
}

#[derive(Debug, Deserialize, Serialize)]
struct ServerConfig {
    host: String,
    port: u16,
    max_logging_level: String,
    #[serde(default = "default_pool_size")]
    pool_size: usize,
    recv_buffer_size: Option<usize>,
    send_buffer_size: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
struct RuntimeConfig {
    #[serde(default = "default_worker_threads")]
    worker_threads: usize,
}

fn default_worker_threads() -> usize {
    1
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Target {
    path: String,
    url: String,
    #[serde(default)]
    retries: Option<RetryConfig>,
    #[serde(default)]
    request_transforms: Option<Vec<Transform>>,
    #[serde(default)]
    response_transforms: Option<Vec<Transform>>,
    #[serde(default)]
    circuit_breaker_config: Option<CircuitBreakerConfig>,
    #[serde(default)]
    rate_limiter_config: Option<RateLimiterConfig>,
    #[serde(default)]
    routing_header: Option<String>,
    #[serde(default)]
    routing_values: Option<HashMap<String, String>>,
    #[serde(default)]
    timeout_seconds: Option<u64>,
    #[serde(default)]
    cache_config: Option<CacheConfig>,
    #[serde(default)]
    logging_config: Option<LoggingConfig>,
    #[serde(default)]
    cors_config: Option<CorsConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct CorsConfig {
    enabled: bool,
    #[serde(rename = "Access-Control-Allow-Origin")]
    allow_origin: Option<String>,
    #[serde(rename = "Access-Control-Allow-Headers")]
    allow_headers: Option<String>,
    #[serde(rename = "Access-Control-Allow-Methods")]
    allow_methods: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct LoggingConfig {
    log_requests: bool,
    log_responses: bool,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct Transform {
    #[serde(rename = "type")]
    transform_type: String,
    name: String,
    #[serde(default)]
    value: Option<String>,
    operation: String,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct SecurityHeadersConfig {
    strict_transport_security: Option<String>,
    x_content_type_options: Option<String>,
    x_frame_options: Option<String>,
    content_security_policy: Option<String>,
    x_xss_protection: Option<String>,
    referrer_policy: Option<String>,
    permissions_policy: Option<String>,
    feature_policy: Option<String>,
}

#[derive(Debug, Clone)]
struct CircuitBreaker {
    state: CircuitState,
    failure_count: usize,
    last_attempt: Instant,
    open_until: Instant,
    reset_timeout: Duration,
    max_failures: usize,
    half_open_attempts: usize,
    half_open_max_attempts: usize,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct CircuitBreakerConfig {
    max_failures: usize,
    reset_timeout_seconds: u64,
    half_open_attempts: usize,
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

impl CircuitBreaker {
    fn new(config: &CircuitBreakerConfig) -> Self {
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

    fn record_success(&mut self) {
        debug!("Circuit breaker for target: Success recorded");
        self.failure_count = 0;
        self.state = CircuitState::Closed;
        self.half_open_attempts = 0;
    }

    fn record_failure(&mut self) {
        debug!("Circuit breaker for target: Failure recorded");
        self.failure_count += 1;
        self.last_attempt = Instant::now();
        if self.failure_count >= self.max_failures {
            self.transition_to_open();
        }
    }

    fn can_attempt(&mut self) -> bool {
        let now = Instant::now();
        match self.state {
            CircuitState::Closed => true,
            CircuitState::Open => {
                if now >= self.open_until {
                    self.transition_to_half_open();
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

    fn transition_to_closed(&mut self) {
        debug!("Circuit breaker transitioning to Closed");
        self.state = CircuitState::Closed;
        self.failure_count = 0;
        self.half_open_attempts = 0;
    }

    fn transition_to_open(&mut self) {
        debug!("Circuit breaker transitioning to Open");
        self.state = CircuitState::Open;
        self.open_until = Instant::now() + self.reset_timeout;
        self.half_open_attempts = 0;
    }

    fn transition_to_half_open(&mut self) {
        debug!("Circuit breaker transitioning to Half-Open");
        self.state = CircuitState::HalfOpen;
        self.half_open_attempts = 0;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CircuitState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
enum RateLimiterConfig {
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
enum RateLimiter {
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
    fn new(config: &RateLimiterConfig, header_value: Option<&str>) -> Self {
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

    async fn acquire(&self) -> bool {
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

struct ProxyState {
    target_map: DashMap<
        String,
        (
            String,
            Option<RetryConfig>,
            Option<Vec<Transform>>,
            Option<Vec<Transform>>,
            Option<CircuitBreakerConfig>,
            Option<RateLimiterConfig>,
            Option<String>,
            Option<HashMap<String, String>>,
            Option<u64>,
            Option<CacheConfig>,
            Option<LoggingConfig>,
            Option<CorsConfig>,
        ),
    >,
    circuit_breakers: DashMap<String, Arc<RwLock<CircuitBreaker>>>,
    rate_limiters: DashMap<String, Arc<RwLock<RateLimiter>>>,
    caches: DashMap<String, Cache>,
    concurrency_limiter: Arc<Semaphore>,
    ongoing_requests: Arc<AtomicUsize>,
}

async fn run_proxy(config: ProxyConfig) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("{}:{}", config.server.host, config.server.port)
        .parse::<SocketAddr>()?;

    let concurrency_limiter = Arc::new(Semaphore::new(config.server.pool_size));
    let ongoing_requests = Arc::new(AtomicUsize::new(0));

    let retry_config = Arc::new(config.retries);
    let default_circuit_breaker_config = Arc::new(config.default_circuit_breaker_config);
    let default_timeout_seconds = config.default_timeout_seconds;
    let default_rate_limiter_config = Arc::new(config.default_rate_limiter_config);
    let security_headers_config = Arc::new(config.security_headers_config.clone());

    let mut http_connector = HttpConnector::new();
    if let Some(recv_buffer_size) = config.server.recv_buffer_size {
        http_connector.set_recv_buffer_size(Some(recv_buffer_size));
    }
    if let Some(send_buffer_size) = config.server.send_buffer_size {
        http_connector.set_send_buffer_size(Some(send_buffer_size));
    }

    let https_connector = HttpsConnector::new();

    let client: Client<HttpsConnector<HttpConnector>> = Client::builder()
        .pool_max_idle_per_host(config.server.pool_size)
        .build(https_connector);

    let initial_target_map = build_target_map(&config.targets);

    let caches = initial_target_map.iter()
        .filter_map(|entry| {
            let (path, (_, _, _, _, _, _, _, _, _, cache_config, _, _)) = entry.pair();
            cache_config.as_ref().map(|config| {
                (path.clone(), Cache::new(config))
            })
        })
        .collect::<DashMap<_, _>>();

    let proxy_state = Arc::new(ProxyState {
        target_map: initial_target_map,
        circuit_breakers: DashMap::new(),
        rate_limiters: DashMap::new(),
        caches,
        concurrency_limiter: concurrency_limiter.clone(),
        ongoing_requests: ongoing_requests.clone(),
    });

    let make_svc = {
        let proxy_state = Arc::clone(&proxy_state);
        let retry_config = Arc::clone(&retry_config);
        let default_circuit_breaker_config = Arc::clone(&default_circuit_breaker_config);
        let default_rate_limiter_config = Arc::clone(&default_rate_limiter_config);
        let client = client.clone();
        let security_headers_config = Arc::clone(&security_headers_config);

        make_service_fn(move |_| {
            let proxy_state = Arc::clone(&proxy_state);
            let retry_config = Arc::clone(&retry_config);
            let default_circuit_breaker_config = Arc::clone(&default_circuit_breaker_config);
            let default_rate_limiter_config = Arc::clone(&default_rate_limiter_config);
            let client = client.clone();
            let security_headers_config = Arc::clone(&security_headers_config);

            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    let proxy_state = Arc::clone(&proxy_state);
                    let retry_config = Arc::clone(&retry_config);
                    let default_circuit_breaker_config = Arc::clone(&default_circuit_breaker_config);
                    let default_rate_limiter_config = Arc::clone(&default_rate_limiter_config);
                    let client = client.clone();
                    let security_headers_config = Arc::clone(&security_headers_config);

                    async move {
                        proxy_state.ongoing_requests.fetch_add(1, Ordering::SeqCst);
                        let response = proxy_request(
                            req,
                            Arc::clone(&proxy_state),
                            retry_config,
                            default_circuit_breaker_config,
                            default_rate_limiter_config,
                            default_timeout_seconds,
                            security_headers_config,
                            client,
                        ).await;
                        proxy_state.ongoing_requests.fetch_sub(1, Ordering::SeqCst);
                        response
                    }
                }))
            }
        })
    };

    let server = Server::bind(&addr).serve(make_svc);
    info!("Proxy is listening on {}", addr);

    let proxy_state_for_shutdown = Arc::clone(&proxy_state);
    let graceful = server.with_graceful_shutdown(async move {
        signal::ctrl_c().await.expect("Failed to capture CTRL+C signal");
        info!("Shutdown signal received, completing pending requests...");

        // Wait until all ongoing requests are completed
        while proxy_state_for_shutdown.ongoing_requests.load(Ordering::SeqCst) > 0 {
            sleep(Duration::from_millis(100)).await;
        }
        info!("All pending requests completed. Shutting down...");
    });

    graceful.await.map_err(Into::into)
}

fn build_target_map(
    targets: &[Target],
) -> DashMap<
    String,
    (
        String,
        Option<RetryConfig>,
        Option<Vec<Transform>>,
        Option<Vec<Transform>>,
        Option<CircuitBreakerConfig>,
        Option<RateLimiterConfig>,
        Option<String>,
        Option<HashMap<String, String>>,
        Option<u64>,
        Option<CacheConfig>,
        Option<LoggingConfig>,
        Option<CorsConfig>,
    ),
> {
    let map = DashMap::new();
    for target in targets {
        map.insert(
            target.path.clone(),
            (
                target.url.clone(),
                target.retries.clone(),
                target.request_transforms.clone(),
                target.response_transforms.clone(),
                target.circuit_breaker_config.clone(),
                target.rate_limiter_config.clone(),
                target.routing_header.clone(),
                target.routing_values.clone(),
                target.timeout_seconds,
                target.cache_config.clone(),
                target.logging_config.clone(),
                target.cors_config.clone(),
            ),
        );
    }
    map
}

async fn proxy_request<C>(
    mut original_req: Request<Body>,
    proxy_state: Arc<ProxyState>,
    default_retry_config: Arc<RetryConfig>,
    default_circuit_breaker_config: Arc<CircuitBreakerConfig>,
    default_rate_limiter_config: Arc<Option<RateLimiterConfig>>,
    default_timeout_seconds: u64,
    security_headers_config: Arc<Option<SecurityHeadersConfig>>,
    client: Client<C>,
) -> Result<Response<Body>, hyper::Error>
where
    C: hyper::client::connect::Connect + Clone + Send + Sync + 'static,
{
    let request_id = Uuid::new_v4();
    info!(request_id = %request_id, "Received request: {:?}", original_req);

    if let Err(validation_error) = validate_request(&mut original_req).await {
        error!(request_id = %request_id, "Request validation failed: {}", validation_error);
        return Ok(ProxyError::BadRequest(validation_error).into());
    }

    let _permit = proxy_state.concurrency_limiter.acquire().await;

    let path = original_req.uri().path().to_string();

    if path == "/health" {
        info!(request_id = %request_id, "Health check request received.");
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Body::from("OK"))
            .unwrap());
    }

    info!(request_id = %request_id, "Handling request for path: {}", path);

    let (
        target_url,
        target_url_len,
        retry_strategy,
        request_transforms,
        response_transforms,
        target_circuit_breaker_config,
        target_rate_limiter_config,
        target_timeout,
        target_cache_config,
        logging_config,
        cors_config,
    ) = match find_target(&proxy_state, &path, &original_req, &default_retry_config, default_timeout_seconds) {
        Some(t) => t,
        None => {
            error!(request_id = %request_id, "No target found for path: {}", path);
            return Ok(ProxyError::NotFound("No target found for the given path".to_string()).into());
        }
    };

    if let Some(config) = &logging_config {
        if config.log_requests {
            info!(request_id = %request_id, "Incoming request: {:?}", original_req);
        }
    }

    let cache_key = create_cache_key(&target_url, &original_req, None);

    if let Some(cache_config) = &target_cache_config {
        if let Some(cache) = proxy_state.caches.get(&cache_key) {
            if let Some(cache_entry) = cache.get(&cache_key).await {
                info!(request_id = %request_id, "Cache hit for: {}", cache_key);
                let vary_headers = cache_entry.vary_headers.clone();
                let new_cache_key = create_cache_key(&target_url, &original_req, vary_headers.as_ref());
                if let Some(entry) = cache.get(&new_cache_key).await {
                    let mut response = Response::builder()
                        .status(entry.status)
                        .body(Body::from(entry.response.clone()))
                        .unwrap();
                    *response.headers_mut() = entry.headers.clone();
                    if let Some(cors_headers) = entry.cors_headers.clone() {
                        response.headers_mut().insert(ACCESS_CONTROL_ALLOW_ORIGIN, cors_headers.allow_origin);
                        response.headers_mut().insert(ACCESS_CONTROL_ALLOW_HEADERS, cors_headers.allow_headers);
                        response.headers_mut().insert(ACCESS_CONTROL_ALLOW_METHODS, cors_headers.allow_methods);
                    }

                    // Add ETag and Last-Modified headers
                    if let Some(etag) = entry.etag.clone() {
                        response.headers_mut().insert("ETag", HeaderValue::from_str(&etag).unwrap());
                    }
                    if let Some(last_modified) = entry.last_modified.clone() {
                        response.headers_mut().insert("Last-Modified", HeaderValue::from_str(&last_modified).unwrap());
                    }

                    apply_security_headers(response.headers_mut(), &security_headers_config).await;

                    if let Some(config) = &logging_config {
                        if config.log_responses {
                            info!(request_id = %request_id, "Outgoing response (from cache): {:?}", response);
                        }
                    }

                    return Ok(response);
                }
            }
        }
    }

    let circuit_breaker_key = format!("{}:{}", target_url, "circuit_breaker");
    let circuit_breaker_lock = proxy_state.circuit_breakers.entry(circuit_breaker_key.clone())
        .or_insert_with(|| Arc::new(RwLock::new(CircuitBreaker::new(target_circuit_breaker_config.as_ref().unwrap_or(&*default_circuit_breaker_config)))));

    {
        let mut circuit_breaker = circuit_breaker_lock.write().await;
        if !circuit_breaker.can_attempt() {
            error!(request_id = %request_id, "Circuit breaker is open for: {}", target_url);
            return Ok(ProxyError::CircuitBreakerOpen.into());
        }
    }

    if let Some(rate_limiter_config) = target_rate_limiter_config.as_ref().or_else(|| default_rate_limiter_config.as_ref().as_ref()) {
        let header_key = match rate_limiter_config {
            RateLimiterConfig::TokenBucket { header_key, .. } |
            RateLimiterConfig::LeakyBucket { header_key, .. } |
            RateLimiterConfig::FixedWindow { header_key, .. } |
            RateLimiterConfig::SlidingLog { header_key, .. } |
            RateLimiterConfig::SlidingWindow { header_key, .. } => {
                header_key.as_deref()
            }
        };

        if let Some(header_key) = header_key {
            if let Some(header_value) = original_req.headers().get(header_key).and_then(|v| v.to_str().ok()) {
                let rate_limiter_key = format!("{}:{}", target_url, header_value);
                let rate_limiter_lock = proxy_state.rate_limiters.entry(rate_limiter_key.clone())
                    .or_insert_with(|| Arc::new(RwLock::new(RateLimiter::new(rate_limiter_config, Some(header_value)))));

                let rate_limiter = rate_limiter_lock.read().await;
                if !rate_limiter.acquire().await {
                    error!(request_id = %request_id, "Rate limit exceeded for: {}", target_url);
                    return Ok(ProxyError::RateLimitExceeded.into());
                }
            }
        }
    }

    // Add conditional request headers
    if let Some(cache) = proxy_state.caches.get(&cache_key) {
        if let Some(cache_entry) = cache.get(&cache_key).await {
            if let Some(etag) = cache_entry.etag.clone() {
                original_req.headers_mut().insert("If-None-Match", HeaderValue::from_str(&etag).unwrap());
            }
            if let Some(last_modified) = cache_entry.last_modified.clone() {
                original_req.headers_mut().insert("If-Modified-Since", HeaderValue::from_str(&last_modified).unwrap());
            }
        }
    }

    let mut retry_strategy = retry_strategy;
    let start_time = Instant::now();
    let retry_statuses = [
        StatusCode::REQUEST_TIMEOUT,
        StatusCode::TOO_MANY_REQUESTS,
        StatusCode::INTERNAL_SERVER_ERROR,
        StatusCode::BAD_GATEWAY,
        StatusCode::SERVICE_UNAVAILABLE,
        StatusCode::GATEWAY_TIMEOUT,
    ];

    let mut retries = 0;
    let max_attempts = retry_strategy.max_attempts();
    while retries < max_attempts {
        let mut req = rebuild_request(&original_req, &target_url, target_url_len);
        if let Some(ref transforms) = request_transforms {
            apply_request_transforms(&mut req, transforms);
        }

        let response_future = client.request(req);
        match timeout(Duration::from_secs(target_timeout), response_future).await {
            Ok(Ok(mut resp)) => {
                if retry_statuses.contains(&resp.status()) {
                    warn!(request_id = %request_id, "Received {} status, retrying request...", resp.status());
                    retries += 1;
                    if retries < max_attempts {
                        sleep(retry_strategy.delay()).await;
                    }
                    continue;
                } else {
                    if let Some(ref transforms) = response_transforms {
                        apply_response_transforms(&mut resp, transforms);
                    }
                    let mut circuit_breaker = circuit_breaker_lock.write().await;
                    if circuit_breaker.state == CircuitState::HalfOpen {
                        circuit_breaker.transition_to_closed();
                    } else {
                        circuit_breaker.record_success();
                    }

                    if resp.status() == StatusCode::NOT_MODIFIED {
                        info!(request_id = %request_id, "Not modified response received, serving from cache");

                        if let Some(cache) = proxy_state.caches.get(&cache_key) {
                            if let Some(cache_entry) = cache.get(&cache_key).await {
                                let mut response = Response::builder()
                                    .status(cache_entry.status)
                                    .body(Body::from(cache_entry.response.clone()))
                                    .unwrap();
                                *response.headers_mut() = cache_entry.headers.clone();
                                if let Some(cors_headers) = cache_entry.cors_headers.clone() {
                                    response.headers_mut().insert(ACCESS_CONTROL_ALLOW_ORIGIN, cors_headers.allow_origin);
                                    response.headers_mut().insert(ACCESS_CONTROL_ALLOW_HEADERS, cors_headers.allow_headers);
                                    response.headers_mut().insert(ACCESS_CONTROL_ALLOW_METHODS, cors_headers.allow_methods);
                                }

                                // Add ETag and Last-Modified headers
                                if let Some(etag) = cache_entry.etag.clone() {
                                    response.headers_mut().insert("ETag", HeaderValue::from_str(&etag).unwrap());
                                }
                                if let Some(last_modified) = cache_entry.last_modified.clone() {
                                    response.headers_mut().insert("Last-Modified", HeaderValue::from_str(&last_modified).unwrap());
                                }

                                apply_security_headers(response.headers_mut(), &security_headers_config).await;
                                return Ok(response);
                            }
                        }
                    } else {
                        if let Some(cache_config) = &target_cache_config {
                            let mut body = std::mem::replace(resp.body_mut(), Body::empty());
                            let response_data = hyper::body::to_bytes(&mut body).await.unwrap_or_else(|_| hyper::body::Bytes::new());

                            let mut cache_entry = CacheEntry::from_response(&mut resp, &cors_config).await;
                            cache_entry.response = response_data.to_vec();
                            let cache_key = create_cache_key(&target_url, &original_req, cache_entry.vary_headers.as_ref());
                            let cache = proxy_state.caches.entry(cache_key.clone()).or_insert_with(|| Cache::new(cache_config));
                            cache.put(cache_key, cache_entry).await;

                            *resp.body_mut() = Body::from(response_data);
                        }

                        if let Some(config) = &logging_config {
                            if config.log_responses {
                                info!(request_id = %request_id, "Outgoing response: {:?}", resp);
                            }
                        }

                        if let Some(cors_config) = &cors_config {
                            if cors_config.enabled {
                                if let Some(allow_origin) = &cors_config.allow_origin {
                                    resp.headers_mut().insert(ACCESS_CONTROL_ALLOW_ORIGIN, HeaderValue::from_str(allow_origin).unwrap());
                                }
                                if let Some(allow_headers) = &cors_config.allow_headers {
                                    resp.headers_mut().insert(ACCESS_CONTROL_ALLOW_HEADERS, HeaderValue::from_str(allow_headers).unwrap());
                                }
                                if let Some(allow_methods) = &cors_config.allow_methods {
                                    resp.headers_mut().insert(ACCESS_CONTROL_ALLOW_METHODS, HeaderValue::from_str(allow_methods).unwrap());
                                }
                            }
                        }

                        apply_security_headers(resp.headers_mut(), &security_headers_config).await;

                        return Ok(resp);
                    }
                }
            }
            Ok(Err(err)) => {
                warn!(request_id = %request_id, "Request failed: {}, retrying...", err);
                retries += 1;
                if retries < max_attempts {
                    sleep(retry_strategy.delay()).await;
                    continue;
                } else {
                    error!(request_id = %request_id, "Error after retries: {}", err);
                    let mut circuit_breaker = circuit_breaker_lock.write().await;
                    circuit_breaker.record_failure();
                    return Ok(ProxyError::InternalServerError(err.to_string()).into());
                }
            }
            Err(_) => {
                if Instant::now().duration_since(start_time) >= Duration::from_secs(target_timeout) {
                    warn!(request_id = %request_id, "Global timeout exceeded, no more retries");
                    let mut circuit_breaker = circuit_breaker_lock.write().await;
                    circuit_breaker.record_failure();
                    return Ok(ProxyError::Timeout.into());
                } else {
                    warn!(request_id = %request_id, "Timeout after {} seconds, retrying...", target_timeout);
                    retries += 1;
                    if retries < max_attempts {
                        sleep(retry_strategy.delay()).await;
                        continue;
                    } else {
                        error!(request_id = %request_id, "Timeout after retries");
                        let mut circuit_breaker = circuit_breaker_lock.write().await;
                        circuit_breaker.record_failure();
                        return Ok(ProxyError::Timeout.into());
                    }
                }
            }
        }
    }

    error!(request_id = %request_id, "Maximum retries exceeded");
    let mut circuit_breaker = circuit_breaker_lock.write().await;
    circuit_breaker.record_failure();
    if circuit_breaker.state == CircuitState::HalfOpen {
        circuit_breaker.transition_to_open();
    }
    Ok(ProxyError::ServiceUnavailable("Maximum retries exceeded".to_string()).into())
}

fn find_target(
    proxy_state: &Arc<ProxyState>,
    path: &str,
    req: &Request<Body>,
    default_retry_config: &Arc<RetryConfig>,
    default_timeout_seconds: u64,
) -> Option<(
    String,
    usize,
    Box<dyn RetryStrategy>,
    Option<Vec<Transform>>,
    Option<Vec<Transform>>,
    Option<CircuitBreakerConfig>,
    Option<RateLimiterConfig>,
    u64,
    Option<CacheConfig>,
    Option<LoggingConfig>,
    Option<CorsConfig>,
)> {
    let target_map = &proxy_state.target_map;
    let mut target = None;
    for entry in target_map.iter() {
        let (p, (
            url,
            retries,
            req_transforms,
            resp_transforms,
            cb_config,
            rate_limiter_config,
            routing_header,
            routing_values,
            t_timeout,
            cache_config,
            log_config,
            cors_config,
        )) = entry.pair();

        if path.starts_with(p) {
            if let Some(header_name) = routing_header {
                if let Some(header_value) = req.headers().get(header_name) {
                    if let Some(target_url) = routing_values.as_ref().and_then(|values| {
                        values.get(header_value.to_str().unwrap_or_default())
                    }) {
                        target = Some((
                            target_url.clone(),
                            p.len(),
                            retries.as_ref().unwrap_or(default_retry_config).to_strategy(),
                            req_transforms.clone(),
                            resp_transforms.clone(),
                            cb_config.clone(),
                            rate_limiter_config.clone(),
                            t_timeout.unwrap_or(default_timeout_seconds),
                            cache_config.clone(),
                            log_config.clone(),
                            cors_config.clone(),
                        ));
                        break;
                    }
                }
            } else {
                target = Some((
                    url.clone(),
                    p.len(),
                    retries.as_ref().unwrap_or(default_retry_config).to_strategy(),
                    req_transforms.clone(),
                    resp_transforms.clone(),
                    cb_config.clone(),
                    rate_limiter_config.clone(),
                    t_timeout.unwrap_or(default_timeout_seconds),
                    cache_config.clone(),
                    log_config.clone(),
                    cors_config.clone(),
                ));
                break;
            }
        }
    }
    target
}

fn rebuild_request(
    original_req: &Request<Body>,
    target_url: &str,
    target_url_len: usize,
) -> Request<Body> {
    let path = original_req.uri().path();
    let query = original_req
        .uri()
        .query()
        .map(|q| format!("?{}", q))
        .unwrap_or_default();
    let new_path = &path[target_url_len..];
    let uri_string = format!("{}{}{}", target_url, new_path, query);
    let uri = uri_string.parse::<Uri>().expect("Invalid URI");
    let authority = uri.authority().map(|auth| auth.to_string());
    let mut builder = Request::builder()
        .method(original_req.method())
        .uri(uri)
        .version(original_req.version());
    if let Some(headers) = builder.headers_mut() {
        for (key, value) in original_req.headers().iter() {
            if key != USER_AGENT {
                headers.insert(key, value.clone());
            }
        }
        if let Some(auth) = authority {
            headers.insert(HOST, HeaderValue::from_str(&auth).unwrap());
        }
    }
    builder
        .body(Body::from(""))
        .expect("Failed to rebuild request")
}

fn apply_request_transforms(req: &mut Request<Body>, transforms: &[Transform]) {
    let headers = req.headers_mut();
    apply_header_transforms(headers, transforms);
}

fn apply_response_transforms(resp: &mut Response<Body>, transforms: &[Transform]) {
    let headers = resp.headers_mut();
    apply_header_transforms(headers, transforms);
}

fn apply_header_transforms(headers: &mut HeaderMap, transforms: &[Transform]) {
    for transform in transforms {
        match transform.operation.as_str() {
            "Set" => {
                if let Ok(header_name) = transform.name.parse::<HeaderName>() {
                    let header_value = transform.value
                        .as_deref()
                        .map_or(Cow::Borrowed(""), Cow::from);
                    headers.insert(header_name, HeaderValue::from_str(&header_value).unwrap_or_else(|_| HeaderValue::from_static("")));
                }
            }
            "Remove" => {
                if let Ok(header_name) = transform.name.parse::<HeaderName>() {
                    headers.remove(header_name);
                }
            }
            "Append" => {
                if let Ok(header_name) = transform.name.parse::<HeaderName>() {
                    if let Some(current_value) = headers.get(&header_name).cloned() {
                        let new_value = format!(
                            "{}{}",
                            current_value.to_str().unwrap_or(""),
                            transform.value.as_ref().unwrap_or(&"".to_string())
                        );
                        if let Ok(new_header_value) = HeaderValue::from_str(&new_value) {
                            headers.insert(header_name, new_header_value);
                        }
                    } else {
                        let header_value = transform.value
                            .as_deref()
                            .map_or(HeaderValue::from_static(""), |v| HeaderValue::from_str(v).unwrap_or_else(|_| HeaderValue::from_static("")));
                        headers.insert(header_name, header_value);
                    }
                }
            }
            _ => warn!("Unknown transform operation: {}", transform.operation),
        }
    }
}

fn main() {
    let config = match read_config() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to read config: {}", e);
            std::process::exit(1);
        }
    };

    let log_level = match config.server.max_logging_level.as_str() {
        "DEBUG" => tracing::Level::DEBUG,
        "INFO" => tracing::Level::INFO,
        "WARN" => tracing::Level::WARN,
        "ERROR" => tracing::Level::ERROR,
        _ => tracing::Level::INFO,
    };

    let (non_blocking_writer, _guard) = non_blocking(std::io::stdout());

    tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_target(false)
        .with_writer(non_blocking_writer)
        .init();

    info!("Starting proxy...");

    let worker_threads = config.runtime.worker_threads;

    let rt = Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .enable_all()
        .build()
        .unwrap();

    rt.block_on(async {
        if let Err(e) = run_proxy(config).await {
            error!("Error running proxy: {}", e);
        }
    });
}

fn read_config() -> Result<ProxyConfig, config::ConfigError> {
    let mut settings = config::Config::default();
    settings.merge(config::File::with_name("config")).unwrap();
    settings.try_into()
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct RetryConfig {
    strategy: String,
    base_delay_seconds: u64,
    max_attempts: usize,
    factor: Option<f64>,
    step_delay_seconds: Option<u64>,
}

impl RetryConfig {
    fn to_strategy(&self) -> Box<dyn RetryStrategy> {
        match self.strategy.as_str() {
            "ExponentialBackoff" => Box::new(ExponentialBackoffStrategy::new(
                Duration::from_secs(self.base_delay_seconds),
                self.factor.unwrap_or(2.0),
                self.max_attempts,
            )),
            "LinearBackoff" => Box::new(LinearBackoffStrategy::new(
                Duration::from_secs(self.base_delay_seconds),
                Duration::from_secs(self.step_delay_seconds.unwrap_or(self.base_delay_seconds)),
                self.max_attempts,
            )),
            "FixedInterval" => Box::new(FixedIntervalBackoffStrategy::new(
                Duration::from_secs(self.base_delay_seconds),
                self.max_attempts,
            )),
            "RandomDelay" => Box::new(RandomDelayStrategy::new(
                Duration::from_secs(self.base_delay_seconds),
                Duration::from_secs(self.step_delay_seconds.unwrap_or(self.base_delay_seconds * 2)),
                self.max_attempts,
            )),
            "IncrementalBackoff" => Box::new(IncrementalBackoffStrategy::new(
                Duration::from_secs(self.base_delay_seconds),
                Duration::from_secs(self.step_delay_seconds.unwrap_or(1)),
                self.max_attempts,
            )),
            "FibonacciBackoff" => Box::new(FibonacciBackoffStrategy::new(
                Duration::from_secs(self.base_delay_seconds),
                self.max_attempts,
            )),
            "GeometricBackoff" => Box::new(GeometricBackoffStrategy::new(
                Duration::from_secs(self.base_delay_seconds),
                self.factor.unwrap_or(2.0),
                self.max_attempts,
            )),
            "HarmonicBackoff" => Box::new(HarmonicBackoffStrategy::new(
                Duration::from_secs(self.base_delay_seconds),
                self.max_attempts,
            )),
            "JitterBackoff" => Box::new(JitterBackoffStrategy::new(
                Box::new(ExponentialBackoffStrategy::new(
                    Duration::from_secs(self.base_delay_seconds),
                    self.factor.unwrap_or(2.0),
                    self.max_attempts
                )),
                0.5,
                self.max_attempts,
            )),
            _ => Box::new(FixedIntervalBackoffStrategy::new(
                Duration::from_secs(self.base_delay_seconds),
                self.max_attempts,
            )),
        }
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct CacheConfig {
    ttl_seconds: u64,
    max_size: usize,
    #[serde(default = "default_serialize")]
    serialize: bool,
}

fn default_serialize() -> bool {
    false
}

#[derive(Debug, Clone)]
struct Cache {
    entries: Arc<DashMap<String, CacheEntry>>,
    ttl: Duration,
    serialize: bool,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    response: Vec<u8>,
    headers: HeaderMap,
    status: StatusCode,
    expires_at: Instant,
    cors_headers: Option<CorsHeaders>,
    etag: Option<String>,
    last_modified: Option<String>,
    vary_headers: Option<Vec<String>>, // Yeni alan
}

#[derive(Debug, Clone)]
struct CorsHeaders {
    allow_origin: HeaderValue,
    allow_headers: HeaderValue,
    allow_methods: HeaderValue,
}

impl CacheEntry {
    async fn from_response(response: &mut Response<Body>, cors_config: &Option<CorsConfig>) -> Self {
        let headers = response.headers().clone();
        let status = response.status();
        let cors_headers = cors_config.as_ref().map(|config| {
            CorsHeaders {
                allow_origin: HeaderValue::from_str(config.allow_origin.as_deref().unwrap_or("*")).unwrap(),
                allow_headers: HeaderValue::from_str(config.allow_headers.as_deref().unwrap_or("*")).unwrap(),
                allow_methods: HeaderValue::from_str(config.allow_methods.as_deref().unwrap_or("GET,POST,PUT,DELETE,OPTIONS")).unwrap(),
            }
        });

        // Extract the body bytes
        let body_bytes = hyper::body::to_bytes(response.body_mut()).await.unwrap_or_else(|_| hyper::body::Bytes::new());

        // Generate ETag
        let etag = headers.get("ETag").map(|v| v.to_str().unwrap().to_string()).or_else(|| {
            let hash = format!("{:x}", md5::compute(&body_bytes));
            Some(format!("\"{}\"", hash))
        });

        // Generate Last-Modified
        let last_modified = headers.get("Last-Modified").map(|v| v.to_str().unwrap().to_string());

        let vary_headers = headers.get(VARY)
            .and_then(|header| header.to_str().ok())
            .map(|header_value| header_value.split(',').map(|s| s.trim().to_string()).collect());

        Self {
            response: body_bytes.to_vec(),
            headers,
            status,
            expires_at: Instant::now(),
            cors_headers,
            etag,
            last_modified,
            vary_headers, // Yeni alan
        }
    }
}

impl Cache {
    fn new(config: &CacheConfig) -> Self {
        let cache = Self {
            entries: Arc::new(DashMap::new()),
            ttl: Duration::from_secs(config.ttl_seconds),
            serialize: config.serialize,
        };

        // Arka planda çalışan temizleme görevi
        let cache_clone = cache.clone();
        tokio::spawn(async move {
            loop {
                cache_clone.clean_expired_entries().await;
                sleep(Duration::from_secs(60)).await; // 60 saniyede bir temizleme işlemi yap
            }
        });

        cache
    }

    async fn get(&self, key: &str) -> Option<CacheEntry> {
        if let Some(entry) = self.entries.get(key) {
            if entry.expires_at > Instant::now() {
                info!("Cache hit for key: {}", key);
                return Some(entry.clone());
            } else {
                warn!("Cache entry expired for key: {}", key);
                self.remove_expired(key).await;
            }
        }
        info!("Cache miss for key: {}", key);
        None
    }

    async fn put(&self, key: String, response: CacheEntry) {
        let entry = CacheEntry {
            response: if self.serialize {
                serde_json::to_vec(&response.response).unwrap_or_else(|_| response.response.clone())
            } else {
                response.response
            },
            headers: response.headers.clone(),
            status: response.status,
            expires_at: Instant::now() + self.ttl,
            cors_headers: response.cors_headers.clone(),
            etag: response.etag.clone(),
            last_modified: response.last_modified.clone(),
            vary_headers: response.vary_headers.clone(), // Yeni alan
        };
        self.entries.insert(key.clone(), entry);
        info!("Cache updated for key: {}", key);
    }

    async fn remove_expired(&self, key: &str) {
        self.entries.remove(key);
        warn!("Cache entry removed for key: {}", key);
    }

    async fn clean_expired_entries(&self) {
        let now = Instant::now();
        let keys_to_remove: Vec<String> = self.entries.iter()
            .filter(|entry| entry.value().expires_at <= now)
            .map(|entry| entry.key().clone())
            .collect();

        for key in keys_to_remove {
            self.remove_expired(&key).await;
        }
    }
}

fn create_cache_key(target_url: &str, req: &Request<Body>, vary_headers: Option<&Vec<String>>) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    target_url.hash(&mut hasher);
    req.uri().to_string().hash(&mut hasher);
    if let Some(headers) = vary_headers {
        for header_name in headers {
            if let Some(value) = req.headers().get(header_name) {
                header_name.hash(&mut hasher);
                value.hash(&mut hasher);
            }
        }
    }
    format!("{:x}", hasher.finish())
}

async fn validate_request(req: &mut Request<Body>) -> Result<(), String> {
    let uri = req.uri().to_string();
    let uri_regex = Regex::new(r"^[a-zA-Z0-9\-._~:/?#\[\]@!$&'()*+,;=%]+$").unwrap();
    if !uri_regex.is_match(&uri) {
        return Err("Invalid URI detected".to_string());
    }

    for (name, value) in req.headers() {
        let header_name_regex = Regex::new(r"^[a-zA-Z0-9\-]+$").unwrap();
        if !header_name_regex.is_match(name.as_str()) {
            return Err(format!("Invalid header name detected: {}", name));
        }

        let header_value_regex = Regex::new(r"^[\x20-\x7E]+$").unwrap();
        if !header_value_regex.is_match(value.to_str().unwrap_or_default()) {
            return Err(format!("Invalid header value detected: {:?}", value));
        }
    }

    let body_bytes = to_bytes(req.body_mut()).await.unwrap();
    let body_content = String::from_utf8_lossy(&body_bytes);
    let sanitized_body = clean(&body_content);

    if body_content != sanitized_body {
        return Err("HTML sanitization failed: potentially harmful content detected".to_string());
    }

    Ok(())
}

#[derive(Debug, Error)]
enum ProxyError {
    #[error("Service Unavailable: Circuit Breaker is Open")]
    CircuitBreakerOpen,
    #[error("Too Many Requests: Rate limit exceeded")]
    RateLimitExceeded,
    #[error("Gateway Timeout")]
    Timeout,
    #[error("Internal Server Error: {0}")]
    InternalServerError(String),
    #[error("Bad Request: {0}")]
    BadRequest(String),
    #[error("Not Found: {0}")]
    NotFound(String),
    #[error("Service Unavailable: {0}")]
    ServiceUnavailable(String),
}

impl From<ProxyError> for Response<Body> {
    fn from(error: ProxyError) -> Self {
        match error {
            ProxyError::CircuitBreakerOpen => Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body(Body::from(error.to_string()))
                .unwrap(),
            ProxyError::RateLimitExceeded => Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .body(Body::from(error.to_string()))
                .unwrap(),
            ProxyError::Timeout => Response::builder()
                .status(StatusCode::GATEWAY_TIMEOUT)
                .body(Body::from(error.to_string()))
                .unwrap(),
            ProxyError::InternalServerError(_) => Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(error.to_string()))
                .unwrap(),
            ProxyError::BadRequest(_) => Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(error.to_string()))
                .unwrap(),
            ProxyError::NotFound(_) => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from(error.to_string()))
                .unwrap(),
            ProxyError::ServiceUnavailable(_) => Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body(Body::from(error.to_string()))
                .unwrap(),
        }
    }
}

async fn apply_security_headers(headers: &mut HeaderMap, security_headers_config: &Option<SecurityHeadersConfig>) {
    if let Some(config) = security_headers_config.as_ref() {
        if let Some(value) = &config.strict_transport_security {
            headers.insert(STRICT_TRANSPORT_SECURITY, HeaderValue::from_str(value).unwrap());
        }
        if let Some(value) = &config.x_content_type_options {
            headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_str(value).unwrap());
        }
        if let Some(value) = &config.x_frame_options {
            headers.insert(X_FRAME_OPTIONS, HeaderValue::from_str(value).unwrap());
        }
        if let Some(value) = &config.content_security_policy {
            headers.insert(CONTENT_SECURITY_POLICY, HeaderValue::from_str(value).unwrap());
        } else {
            // Default CSP
            let default_csp = "default-src 'self'; script-src 'self'; object-src 'none'; style-src 'self'; img-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'";
            headers.insert(CONTENT_SECURITY_POLICY, HeaderValue::from_str(default_csp).unwrap());
        }
        if let Some(value) = &config.x_xss_protection {
            headers.insert(X_XSS_PROTECTION, HeaderValue::from_str(value).unwrap());
        } else {
            // Default X-XSS-Protection
            headers.insert(X_XSS_PROTECTION, HeaderValue::from_static("1; mode=block"));
        }
        if let Some(value) = &config.referrer_policy {
            headers.insert(REFERRER_POLICY, HeaderValue::from_str(value).unwrap());
        } else {
            // Default Referrer-Policy
            headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
        }
        if let Some(value) = &config.permissions_policy {
            headers.insert(PERMISSIONS_POLICY, HeaderValue::from_str(value).unwrap());
        }
        if let Some(value) = &config.feature_policy {
            headers.insert(FEATURE_POLICY, HeaderValue::from_str(value).unwrap());
        }
        info!("Applied security headers: {:?}", headers);
    }
}
