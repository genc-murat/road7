use hyper::{Body, Client, Request, Response, Server, StatusCode, Uri};
use hyper::service::{make_service_fn, service_fn};
use hyper::header::{HeaderValue, HOST, USER_AGENT};
use hyper::client::HttpConnector;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore, Mutex};
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Duration, timeout};
use tracing::{debug, error, info, warn};
use tracing_subscriber;
use std::convert::Infallible;
use tokio::signal;
use std::time::Instant;
use std::net::SocketAddr;
use rand::{Rng, thread_rng};
use lru::LruCache;
use std::hash::{Hash, Hasher};

#[derive(Debug, Deserialize, Serialize)]
struct ProxyConfig {
    server: ServerConfig,
    targets: Vec<Target>,
    retries: RetryConfig,
    #[serde(default)]
    default_circuit_breaker_config: CircuitBreakerConfig,
    #[serde(default = "default_timeout_seconds")]
    default_timeout_seconds: u64, // Default timeout in seconds for all targets
    #[serde(default)]
    default_rate_limiter_config: Option<RateLimiterConfig>, // Optional default rate limiter
}

fn default_timeout_seconds() -> u64 {
    30 // Default timeout of 30 seconds
}

#[derive(Debug, Deserialize, Serialize)]
struct ServerConfig {
    host: String,
    port: u16,
    max_logging_level: String,
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
    logging_config: Option<LoggingConfig>, // Optional logging configuration
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct LoggingConfig {
    log_requests: bool, // Whether to log incoming requests
    log_responses: bool, // Whether to log outgoing responses
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
            refill_rate: 10, // 10 requests per second
            burst_capacity: 20, // Allow bursts of up to 20 requests
            header_key: None, // No dynamic rate limiting by default
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
                    // Reset the window
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
                // Remove requests outside the window
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
                    // Slide the window
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
    target_map: HashMap<
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
        ),
    >,
    circuit_breakers: RwLock<HashMap<String, RwLock<CircuitBreaker>>>,
    rate_limiters: RwLock<HashMap<String, RwLock<RateLimiter>>>,
    caches: RwLock<HashMap<String, Cache>>,
}

async fn run_proxy(config: ProxyConfig) -> Result<(), Box<dyn std::error::Error>> {
    let addr = format!("{}:{}", config.server.host, config.server.port)
        .parse::<SocketAddr>()?;

    let retry_config = Arc::new(config.retries);
    let default_circuit_breaker_config = Arc::new(config.default_circuit_breaker_config);
    let default_timeout_seconds = config.default_timeout_seconds;
    let default_rate_limiter_config = Arc::new(config.default_rate_limiter_config);

    let client = Client::builder()
        .pool_max_idle_per_host(5)
        .build_http::<Body>();

    let initial_target_map = build_target_map(&config.targets);

    let caches = RwLock::new(
        initial_target_map.iter()
            .filter_map(|(path, (_, _, _, _, _, _, _, _, _, cache_config, _))| {
                cache_config.as_ref().map(|config| {
                    (path.clone(), Cache::new(config))
                })
            })
            .collect::<HashMap<_, _>>()
    );

    let proxy_state = Arc::new(ProxyState {
        target_map: initial_target_map,
        circuit_breakers: RwLock::new(HashMap::new()),
        rate_limiters: RwLock::new(HashMap::new()),
        caches,
    });

    let make_svc = make_service_fn(move |_| {
        let proxy_state = Arc::clone(&proxy_state);
        let retry_config = Arc::clone(&retry_config);
        let default_circuit_breaker_config = Arc::clone(&default_circuit_breaker_config);
        let default_rate_limiter_config = Arc::clone(&default_rate_limiter_config);
        let client = client.clone();
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                proxy_request(
                    req,
                    Arc::clone(&proxy_state),
                    Arc::clone(&retry_config),
                    Arc::clone(&default_circuit_breaker_config),
                    Arc::clone(&default_rate_limiter_config),
                    default_timeout_seconds,
                    client.clone(),
                )
            }))
        }
    });

    let server = Server::bind(&addr).serve(make_svc);
    info!("Proxy is listening on http://{}", addr);
    let graceful = server.with_graceful_shutdown(shutdown_signal());
    graceful.await.map_err(Into::into)
}

async fn shutdown_signal() {
    signal::ctrl_c()
        .await
        .expect("Failed to capture CTRL+C signal");
    info!("Shutdown signal received, completing pending requests...");
}

fn build_target_map(
    targets: &[Target],
) -> HashMap<
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
    ),
> {
    let mut map = HashMap::new();
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
            ),
        );
    }
    map
}

async fn proxy_request(
    original_req: Request<Body>,
    proxy_state: Arc<ProxyState>,
    default_retry_config: Arc<RetryConfig>,
    default_circuit_breaker_config: Arc<CircuitBreakerConfig>,
    default_rate_limiter_config: Arc<Option<RateLimiterConfig>>,
    default_timeout_seconds: u64,
    client: Client<HttpConnector>,
) -> Result<Response<Body>, hyper::Error> {
    let path = original_req.uri().path().to_string();
    info!("Handling request for path: {}", path);

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
    ) = {
        let target_map = &proxy_state.target_map;
        let mut target = None;
        for (
            p,
            (
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
            ),
        ) in target_map.iter()
        {
            if path.starts_with(p) {
                if let Some(header_name) = routing_header {
                    if let Some(header_value) = original_req.headers().get(header_name) {
                        if let Some(target_url) = routing_values.as_ref().and_then(|values| {
                            values.get(header_value.to_str().unwrap_or_default())
                        }) {
                            target = Some((
                                target_url.clone(),
                                p.len(),
                                retries.as_ref().unwrap_or(&*default_retry_config).to_strategy(),
                                req_transforms.clone(),
                                resp_transforms.clone(),
                                cb_config.clone(),
                                rate_limiter_config.clone(),
                                t_timeout.unwrap_or(default_timeout_seconds),
                                cache_config.clone(),
                                log_config.clone(),
                            ));
                            break;
                        }
                    }
                } else {
                    target = Some((
                        url.clone(),
                        p.len(),
                        retries.as_ref().unwrap_or(&*default_retry_config).to_strategy(),
                        req_transforms.clone(),
                        resp_transforms.clone(),
                        cb_config.clone(),
                        rate_limiter_config.clone(),
                        t_timeout.unwrap_or(default_timeout_seconds),
                        cache_config.clone(),
                        log_config.clone(),
                    ));
                    break;
                }
            }
        }
        match target {
            Some(t) => t,
            None => {
                error!("No target found for path: {}", path);
                return Ok(
                    Response::builder()
                        .status(StatusCode::NOT_FOUND)
                        .body(Body::from("404 Not Found"))
                        .unwrap(),
                );
            }
        }
    };

    // Log incoming request if logging is enabled
    if let Some(config) = &logging_config {
        if config.log_requests {
            info!("Incoming request: {:?}", original_req);
        }
    }

    // Create a unique cache key based on the request and target URL
    let cache_key = create_cache_key(&target_url, &original_req);

    // Attempt to retrieve a response from the cache if caching is configured
    if let Some(cache_config) = &target_cache_config {
        let caches = proxy_state.caches.read().await;
        if let Some(cache) = caches.get(&cache_key) {
            if let Some(cached_response) = cache.get(&cache_key).await {
                info!("Cache hit for: {}", cache_key);
                return Ok(Response::builder().status(StatusCode::OK).body(Body::from(cached_response)).unwrap());
            }
        }
    }

    // Circuit breaker handling
    let circuit_breaker_key = format!("{}:{}", target_url, "circuit_breaker");
    let mut circuit_breakers = proxy_state.circuit_breakers.write().await;
    let circuit_breaker_lock = circuit_breakers.entry(circuit_breaker_key.clone())
        .or_insert_with(|| RwLock::new(CircuitBreaker::new(target_circuit_breaker_config.as_ref().unwrap_or(&*default_circuit_breaker_config))));

    {
        let mut circuit_breaker = circuit_breaker_lock.write().await;
        if !circuit_breaker.can_attempt() {
            error!("Circuit breaker is open for: {}", target_url);
            return Ok(Response::builder().status(StatusCode::SERVICE_UNAVAILABLE).body(Body::from("Service Unavailable: Circuit Breaker is Open")).unwrap());
        }
    }

    // Rate limiter handling
    if let Some(rate_limiter_config) = target_rate_limiter_config.as_ref().or_else(|| default_rate_limiter_config.as_ref().as_ref()) {
        let header_key = match rate_limiter_config {
            RateLimiterConfig::TokenBucket { header_key, .. } |
            RateLimiterConfig::LeakyBucket { header_key, .. } |
            RateLimiterConfig::FixedWindow { header_key, .. } |
            RateLimiterConfig::SlidingLog { header_key, .. } |
            RateLimiterConfig::SlidingWindow { header_key, .. } => {
                header_key.as_deref()
            },
            _ => None,
        };

        if let Some(header_key) = header_key {
            if let Some(header_value) = original_req.headers().get(header_key).and_then(|v| v.to_str().ok()) {
                let rate_limiter_key = format!("{}:{}", target_url, header_value);
                let mut rate_limiters = proxy_state.rate_limiters.write().await;
                let rate_limiter_lock = rate_limiters.entry(rate_limiter_key.clone())
                    .or_insert_with(|| RwLock::new(RateLimiter::new(rate_limiter_config, Some(header_value))));

                let rate_limiter = rate_limiter_lock.read().await;
                if !rate_limiter.acquire().await {
                    error!("Rate limit exceeded for: {}", target_url);
                    return Ok(Response::builder().status(StatusCode::TOO_MANY_REQUESTS).body(Body::from("Too Many Requests: Rate limit exceeded")).unwrap());
                }
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
                    warn!("Received {} status, retrying request...", resp.status());
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

                    // Cache the response if caching is configured
                    if let Some(cache_config) = &target_cache_config {
                        let mut body = std::mem::replace(resp.body_mut(), Body::empty());
                        let response_data = hyper::body::to_bytes(&mut body).await.unwrap_or_else(|_| hyper::body::Bytes::new());

                        let mut caches = proxy_state.caches.write().await;
                        let cache = caches.entry(cache_key.clone()).or_insert_with(|| Cache::new(cache_config));
                        cache.put(cache_key, response_data.to_vec()).await;

                        *resp.body_mut() = Body::from(response_data);
                    }

                    // Log outgoing response if logging is enabled
                    if let Some(config) = &logging_config {
                        if config.log_responses {
                            info!("Outgoing response: {:?}", resp);
                        }
                    }

                    return Ok(resp);
                }
            }
            Ok(Err(err)) => {
                warn!("Request failed: {}, retrying...", err);
                retries += 1;
                if retries < max_attempts {
                    sleep(retry_strategy.delay()).await;
                    continue;
                } else {
                    error!("Error after retries: {}", err);
                    let mut circuit_breaker = circuit_breaker_lock.write().await;
                    circuit_breaker.record_failure();
                    return Ok(Response::builder().status(StatusCode::BAD_GATEWAY).body(Body::from(format!("Bad Gateway: {}", err))).unwrap());
                }
            }
            Err(_) => {
                if Instant::now().duration_since(start_time) >= Duration::from_secs(target_timeout) {
                    warn!("Global timeout exceeded, no more retries");
                    let mut circuit_breaker = circuit_breaker_lock.write().await;
                    circuit_breaker.record_failure();
                    return Ok(Response::builder().status(StatusCode::GATEWAY_TIMEOUT).body(Body::from("Gateway Timeout")).unwrap());
                } else {
                    warn!("Timeout after {} seconds, retrying...", target_timeout);
                    retries += 1;
                    if retries < max_attempts {
                        sleep(retry_strategy.delay()).await;
                        continue;
                    } else {
                        error!("Timeout after retries");
                        let mut circuit_breaker = circuit_breaker_lock.write().await;
                        circuit_breaker.record_failure();
                        return Ok(Response::builder().status(StatusCode::GATEWAY_TIMEOUT).body(Body::from("Gateway Timeout")).unwrap());
                    }
                }
            }
        }
    }

    error!("Maximum retries exceeded");
    let mut circuit_breaker = circuit_breaker_lock.write().await;
    circuit_breaker.record_failure();
    if circuit_breaker.state == CircuitState::HalfOpen {
        circuit_breaker.transition_to_open();
    }
    Ok(Response::builder().status(StatusCode::SERVICE_UNAVAILABLE).body(Body::from("Service Unavailable: Maximum retries exceeded")).unwrap())
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

fn apply_header_transforms(
    headers: &mut http::HeaderMap,
    transforms: &[Transform],
) {
    for transform in transforms {
        match transform.operation.as_str() {
            "Set" => {
                if let Ok(header_name) = transform.name.parse::<http::header::HeaderName>()
                {
                    let header_value = match transform.value.as_ref() {
                        Some(value) => {
                            HeaderValue::from_str(value).unwrap_or_else(|_| {
                                HeaderValue::from_static("")
                            })
                        }
                        None => HeaderValue::from_static(""),
                    };
                    headers.insert(header_name, header_value);
                }
            }
            "Remove" => {
                if let Ok(header_name) = transform.name.parse::<http::header::HeaderName>()
                {
                    headers.remove(header_name);
                }
            }
            "Append" => {
                if let Ok(header_name) = transform.name.parse::<http::header::HeaderName>()
                {
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
                        let header_value = HeaderValue::from_str(
                            &transform.value.as_ref().unwrap_or(&"".to_string()),
                        )
                        .unwrap_or_else(|_| HeaderValue::from_static(""));
                        headers.insert(header_name, header_value);
                    }
                }
            }
            _ => warn!("Unknown transform operation: {}", transform.operation),
        }
    }
}

#[tokio::main]
async fn main() {
    // Read the configuration
    let config = match read_config() {
        Ok(config) => config,
        Err(e) => {
            eprintln!("Failed to read config: {}", e);
            std::process::exit(1);
        }
    };

    // Initialize the logger based on the config
    let log_level = match config.server.max_logging_level.as_str() {
        "DEBUG" => tracing::Level::DEBUG,
        "INFO" => tracing::Level::INFO,
        "WARN" => tracing::Level::WARN,
        "ERROR" => tracing::Level::ERROR,
        _ => tracing::Level::INFO,
    };

    tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_target(false)
        .init();

    info!("Starting proxy...");

    if let Err(e) = run_proxy(config).await {
        error!("Error running proxy: {}", e);
    }
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

/// Trait for defining different retry strategies.
pub trait RetryStrategy: Send + Sync {
    fn delay(&mut self) -> Duration;
    fn max_attempts(&self) -> usize;
}

/// Fixed Interval Backoff Strategy
pub struct FixedIntervalBackoffStrategy {
    base_delay: Duration,
    current_attempt: usize,
    max_attempts: usize,
}

impl FixedIntervalBackoffStrategy {
    pub fn new(base_delay: Duration, max_attempts: usize) -> Self {
        Self {
            base_delay,
            current_attempt: 1,
            max_attempts,
        }
    }
}

impl RetryStrategy for FixedIntervalBackoffStrategy {
    fn delay(&mut self) -> Duration {
        // Always returns the same delay, does not change with attempts
        self.base_delay
    }

    fn max_attempts(&self) -> usize {
        self.max_attempts
    }
}

/// Exponential Backoff Strategy
pub struct ExponentialBackoffStrategy {
    base_delay: Duration,
    current_attempt: usize,
    factor: f64,
    max_attempts: usize,
}

impl ExponentialBackoffStrategy {
    pub fn new(base_delay: Duration, factor: f64, max_attempts: usize) -> Self {
        Self {
            base_delay,
            current_attempt: 1,
            factor,
            max_attempts,
        }
    }
}

impl RetryStrategy for ExponentialBackoffStrategy {
    fn delay(&mut self) -> Duration {
        let delay = self.base_delay.mul_f64(self.factor.powf(self.current_attempt as f64));
        self.current_attempt += 1;
        delay
    }

    fn max_attempts(&self) -> usize {
        self.max_attempts
    }
}

/// Linear Backoff Strategy
pub struct LinearBackoffStrategy {
    base_delay: Duration,
    step: Duration,
    current_attempt: usize,
    max_attempts: usize,
}

impl LinearBackoffStrategy {
    pub fn new(base_delay: Duration, step: Duration, max_attempts: usize) -> Self {
        Self {
            base_delay,
            step,
            current_attempt: 1,
            max_attempts,
        }
    }
}

impl RetryStrategy for LinearBackoffStrategy {
    fn delay(&mut self) -> Duration {
        let delay = self.base_delay + self.step * (self.current_attempt as u32);
        self.current_attempt += 1;
        delay
    }

    fn max_attempts(&self) -> usize {
        self.max_attempts
    }
}

/// Random Delay Strategy
pub struct RandomDelayStrategy {
    min_delay: Duration,
    max_delay: Duration,
    max_attempts: usize,
}

impl RandomDelayStrategy {
    pub fn new(min_delay: Duration, max_delay: Duration, max_attempts: usize) -> Self {
        Self {
            min_delay,
            max_delay,
            max_attempts,
        }
    }
}

impl RetryStrategy for RandomDelayStrategy {
    fn delay(&mut self) -> Duration {
        let mut rng = thread_rng();
        let secs = rng.gen_range(self.min_delay.as_secs()..self.max_delay.as_secs());
        let nanos = rng.gen_range(0..1_000_000_000);
        Duration::new(secs, nanos as u32)
    }

    fn max_attempts(&self) -> usize {
        self.max_attempts
    }
}

/// Incremental Backoff Strategy
pub struct IncrementalBackoffStrategy {
    initial_delay: Duration,
    increment: Duration,
    current_attempt: usize,
    max_attempts: usize,
}

impl IncrementalBackoffStrategy {
    pub fn new(initial_delay: Duration, increment: Duration, max_attempts: usize) -> Self {
        Self {
            initial_delay,
            increment,
            current_attempt: 1,
            max_attempts,
        }
    }
}

impl RetryStrategy for IncrementalBackoffStrategy {
    fn delay(&mut self) -> Duration {
        let delay = self.initial_delay + self.increment * (self.current_attempt as u32);
        self.current_attempt += 1;
        delay
    }

    fn max_attempts(&self) -> usize {
        self.max_attempts
    }
}

/// Fibonacci Backoff Strategy
pub struct FibonacciBackoffStrategy {
    base_delay: Duration,
    current: u64,
    next: u64,
    max_attempts: usize,
}

impl FibonacciBackoffStrategy {
    pub fn new(base_delay: Duration, max_attempts: usize) -> Self {
        Self {
            base_delay,
            current: 1,
            next: 1,
            max_attempts,
        }
    }
}

impl RetryStrategy for FibonacciBackoffStrategy {
    fn delay(&mut self) -> Duration {
        let delay_seconds = self.base_delay.as_secs() * self.current;
        let new_next = self.current + self.next;
        self.current = self.next;
        self.next = new_next;

        Duration::from_secs(delay_seconds)
    }

    fn max_attempts(&self) -> usize {
        self.max_attempts
    }
}

/// Geometric Backoff Strategy
pub struct GeometricBackoffStrategy {
    base_delay: Duration,
    factor: f64,
    current_attempt: usize,
    max_attempts: usize,
}

impl GeometricBackoffStrategy {
    pub fn new(base_delay: Duration, factor: f64, max_attempts: usize) -> Self {
        Self {
            base_delay,
            factor,
            current_attempt: 1,
            max_attempts,
        }
    }
}

impl RetryStrategy for GeometricBackoffStrategy {
    fn delay(&mut self) -> Duration {
        let delay = self.base_delay.mul_f64(self.factor.powf(self.current_attempt as f64));
        self.current_attempt += 1;
        delay
    }

    fn max_attempts(&self) -> usize {
        self.max_attempts
    }
}

/// Harmonic Backoff Strategy
pub struct HarmonicBackoffStrategy {
    base_delay: Duration,
    current_attempt: usize,
    max_attempts: usize,
}

impl HarmonicBackoffStrategy {
    pub fn new(base_delay: Duration, max_attempts: usize) -> Self {
        Self {
            base_delay,
            current_attempt: 1,
            max_attempts,
        }
    }
}

impl RetryStrategy for HarmonicBackoffStrategy {
    fn delay(&mut self) -> Duration {
        let divisor = self.current_attempt.try_into().unwrap_or(u32::MAX);
        let delay = self.base_delay / divisor;
        self.current_attempt += 1;
        delay
    }

    fn max_attempts(&self) -> usize {
        self.max_attempts
    }
}

/// Jitter Backoff Strategy
pub struct JitterBackoffStrategy {
    strategy: Box<dyn RetryStrategy>,
    jitter_factor: f64,
    max_attempts: usize,
}

impl JitterBackoffStrategy {
    pub fn new(strategy: Box<dyn RetryStrategy>, jitter_factor: f64, max_attempts: usize) -> Self {
        Self {
            strategy,
            jitter_factor,
            max_attempts,
        }
    }
}

impl RetryStrategy for JitterBackoffStrategy {
    fn delay(&mut self) -> Duration {
        let base_delay = self.strategy.delay();
        let jitter = base_delay.mul_f64(self.jitter_factor);
        let mut rng = thread_rng();
        let secs = rng.gen_range(0..jitter.as_secs());
        let nanos = rng.gen_range(0..1_000_000_000);
        base_delay + Duration::new(secs, nanos as u32)
    }

    fn max_attempts(&self) -> usize {
        self.max_attempts
    }
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct CacheConfig {
    ttl_seconds: u64, // Time to live: Duration in seconds that the cache is valid
    max_size: usize, // Maximum size of the cache (number of entries)
    #[serde(default = "default_serialize")]
    serialize: bool, // Whether to serialize data in the cache
}

fn default_serialize() -> bool {
    false
}

#[derive(Debug, Clone)]
struct Cache {
    entries: Arc<Mutex<LruCache<String, CacheEntry>>>,
    ttl: Duration,
    serialize: bool,
}

#[derive(Debug, Clone)]
struct CacheEntry {
    response: Vec<u8>,
    expires_at: Instant,
}

impl Cache {
    fn new(config: &CacheConfig) -> Self {
        Self {
            entries: Arc::new(Mutex::new(LruCache::new(config.max_size))),
            ttl: Duration::from_secs(config.ttl_seconds),
            serialize: config.serialize,
        }
    }
    async fn get(&self, key: &str) -> Option<Vec<u8>> {
        let mut entries = self.entries.lock().await;
        if let Some(entry) = entries.get(key) {
            if entry.expires_at > Instant::now() {
                info!("Cache hit for key: {}", key);
                return Some(entry.response.clone());
            } else {
                warn!("Cache entry expired for key: {}", key);
                entries.pop(key);
            }
        }
        info!("Cache miss for key: {}", key);
        None
    }

    async fn put(&self, key: String, response: Vec<u8>) {
        let mut entries = self.entries.lock().await;
        let entry = CacheEntry {
            response: if self.serialize {
                serde_json::to_vec(&response).unwrap_or_else(|_| response.clone())
            } else {
                response
            },
            expires_at: Instant::now() + self.ttl,
        };
        entries.put(key.clone(), entry);
        info!("Cache updated for key: {}", key);
    }
}

fn create_cache_key(target_url: &str, req: &Request<Body>) -> String {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    target_url.hash(&mut hasher);
    req.uri().to_string().hash(&mut hasher);
    req.headers().iter().for_each(|(key, value)| {
        key.hash(&mut hasher);
        value.hash(&mut hasher);
    });
    format!("{:x}", hasher.finish())
}
