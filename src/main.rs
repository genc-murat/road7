mod auth {
    pub mod jwt;
    pub mod basic;
}

mod retry;
mod transform;
mod error;
mod circuit_breaker;
mod metrics;
mod cache;
mod validate;
mod security_headers;
mod load_balancer;
mod bot_detector;
mod config;
mod rate_limiter;
mod state;

use state::{ProxyState, ProxyStateBuilder, increment_counter_async, increment_counter_with_label_async, observe_histogram_async, set_gauge_async, serve_static_file, rate_limiter_status_handler};
use crate::cache::{Cache, CacheConfig, create_cache_key, CacheEntry};
use crate::config::{read_config, AuthenticationType, AuthenticationConfig, LoggingConfig, ProxyConfig};
use crate::load_balancer::LoadBalancer;
use crate::auth::jwt::validate_jwt;
use crate::auth::basic::validate_basic;
use crate::rate_limiter::{RateLimitError, RateLimiterConfig};
use crate::security_headers::apply_security_headers;
use crate::validate::validate_request;
use crate::bot_detector::is_bot_request;
use crate::metrics::Metrics;
use cache::CorsConfig;
use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitState};

use error::ProxyError;
use hyper::server::conn::AddrStream;
use rate_limiter::RateLimiterManager;
use retry::{RetryConfig, RetryStrategy};
use security_headers::SecurityHeadersConfig;
use transform::Transform;
use hyper::{Body, Client, Request, Response, Server, StatusCode, Uri};
use hyper::service::{make_service_fn, service_fn};
use hyper::header::{HeaderValue, ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, HOST};
use hyper::client::HttpConnector;
use hyper_tls::HttpsConnector;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};
use tokio::time::{sleep, Duration, timeout};
use tracing::{error, info, warn};
use tracing_subscriber;
use std::convert::Infallible;
use tokio::signal;
use std::time::Instant;
use std::net::SocketAddr;
use tracing_appender::non_blocking;
use dashmap::DashMap;
use uuid::Uuid;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::runtime::Builder;

const POOL_IDLE_TIMEOUT: u64 = 30;

async fn run_proxy(config: ProxyConfig) -> Result<(), Box<dyn std::error::Error>> {
    let metrics = Arc::new(Metrics::new());
    let concurrency_limiter = Arc::new(Semaphore::new(config.server.pool_size));
    let ongoing_requests = Arc::new(AtomicUsize::new(0));

    let initial_target_map = state::build_target_map(&config.targets);

    let caches = initial_target_map.iter()
        .filter_map(|entry| {
            let (path, (_, _, _, _, _, _, _, _, cache_config, _, _, _)) = entry.pair();
            cache_config.as_ref().map(|config| {
                (path.clone(), Cache::new(config))
            })
        })
        .collect::<DashMap<_, _>>();

    let load_balancer = config.load_balancer.as_ref().map(|lb_config| {
        Arc::new(RwLock::new(LoadBalancer::new(
            config.targets.iter().map(|t| (t.path.clone(), t.urls.clone())).collect(),
            lb_config.algorithm.clone(),
            lb_config.weights.clone(),
        )))
    });

    let bot_detector = config.bot_detector.as_ref().map(|bot_config| Arc::new(bot_config.clone()));

    let rate_limiters = Arc::new(RateLimiterManager::new());

    for target in &config.targets {
        if let Some(rate_limiter_config) = &target.rate_limiter_config {
            rate_limiters.add_limiter(
                target.path.clone(),
                RateLimiterConfig {
                    capacity: rate_limiter_config.capacity,
                    burst_capacity: rate_limiter_config.burst_capacity,
                    max_rate: rate_limiter_config.max_rate,
                    period: Duration::from_secs(rate_limiter_config.every.unwrap_or(1)),
                },
            ).await;
        }

        if let Some(endpoints) = &target.endpoints {
            for endpoint in endpoints {
                if let Some(rate_limiter_config) = &endpoint.rate_limiter_config {
                    let full_path = format!("{}{}", target.path, endpoint.path);
                    rate_limiters.add_limiter(
                        full_path,
                        RateLimiterConfig {
                            capacity: rate_limiter_config.capacity,
                            burst_capacity: rate_limiter_config.burst_capacity,
                            max_rate: rate_limiter_config.max_rate,
                            period: Duration::from_secs(rate_limiter_config.every.unwrap_or(1)),
                        },
                    ).await;
                }
            }
        }
    }

    let proxy_state = Arc::new(ProxyStateBuilder::new()
        .with_target_map(&config.targets)
        .with_circuit_breakers(DashMap::new())
        .with_caches(caches)
        .with_concurrency_limiter(concurrency_limiter.clone())
        .with_ongoing_requests(ongoing_requests.clone())
        .with_metrics(metrics.clone())
        .with_default_cors_config(config.default_cors_config.clone())
        .with_load_balancer(load_balancer)
        .with_bot_detector(bot_detector)
        .with_rate_limiters(rate_limiters)
        .build()?
    );

    let addr = format!("{}:{}", config.server.host, config.server.port)
        .parse::<SocketAddr>()?;

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
        .pool_idle_timeout(Duration::from_secs(POOL_IDLE_TIMEOUT))
        .build(https_connector);

    let make_svc = {
        let proxy_state = Arc::clone(&proxy_state);
        let retry_config = Arc::new(config.retries);
        let default_circuit_breaker_config = Arc::new(config.default_circuit_breaker_config);
        let default_timeout_seconds = config.default_timeout_seconds;
        let security_headers_config = Arc::new(config.security_headers_config.clone());

        make_service_fn(move |conn: &AddrStream| {
            let proxy_state = Arc::clone(&proxy_state);
            let retry_config = Arc::clone(&retry_config);
            let default_circuit_breaker_config = Arc::clone(&default_circuit_breaker_config);
            let client = client.clone();
            let security_headers_config = Arc::clone(&security_headers_config);

            let client_ip = conn.remote_addr();

            async move {
                Ok::<_, Infallible>(service_fn(move |req| {
                    let proxy_state = Arc::clone(&proxy_state);
                    let retry_config = Arc::clone(&retry_config);
                    let default_circuit_breaker_config = Arc::clone(&default_circuit_breaker_config);
                    let client = client.clone();
                    let security_headers_config = Arc::clone(&security_headers_config);

                    async move {
                        proxy_state.ongoing_requests.fetch_add(1, Ordering::SeqCst);
                        proxy_state.metrics.http_requests_total.inc();
                        proxy_state.metrics.ongoing_requests.inc();
                        proxy_state.metrics.http_method_counts.with_label_values(&[req.method().as_str()]).inc();

                        if let Some(bot_detector) = &proxy_state.bot_detector {
                            if is_bot_request(&req, bot_detector).await {
                                warn!("Bot detected and request rejected: {:?}", req);
                                increment_counter_async(proxy_state.metrics.failed_requests.clone()).await;
                                increment_counter_with_label_async(proxy_state.metrics.error_counts.clone(), "bot_detected".to_string()).await;
                                return Ok(ProxyError::Forbidden("Bot detected".to_string()).into());
                            }
                        }
                        

                        let response = proxy_request(
                            req,
                            client_ip,
                            Arc::clone(&proxy_state),
                            retry_config,
                            default_circuit_breaker_config,
                            default_timeout_seconds,
                            security_headers_config,
                            client,
                        ).await;
                        proxy_state.ongoing_requests.fetch_sub(1, Ordering::SeqCst);
                        proxy_state.metrics.ongoing_requests.dec();
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

        while proxy_state_for_shutdown.ongoing_requests.load(Ordering::SeqCst) > 0 {
            sleep(Duration::from_millis(100)).await;
        }
        proxy_state_for_shutdown.shutdown().await;
        info!("All pending requests completed. Shutting down...");
    });

    graceful.await.map_err(Into::into)
}

async fn proxy_request<C>(
    mut original_req: Request<Body>,
    client_ip: SocketAddr,
    proxy_state: Arc<ProxyState>,
    default_retry_config: Arc<RetryConfig>,
    default_circuit_breaker_config: Arc<CircuitBreakerConfig>,
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
        increment_counter_async(proxy_state.metrics.failed_requests.clone()).await;
        increment_counter_with_label_async(proxy_state.metrics.error_counts.clone(), "validation_error".to_string()).await;
        return Ok(ProxyError::BadRequest(validation_error).into());
    }

    let request_size = hyper::body::to_bytes(original_req.body_mut()).await.unwrap().len();
    observe_histogram_async(proxy_state.metrics.request_size_bytes.clone(), request_size as f64).await;

    let _permit = proxy_state.concurrency_limiter.acquire().await;

    let path = original_req.uri().path().to_string();

    if path == "/health" {
        info!(request_id = %request_id, "Health check request received.");
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .body(Body::from("OK"))
            .unwrap());
    }

    if path == "/metrics" {
        info!(request_id = %request_id, "Metrics request received.");
        let metrics_data = proxy_state.metrics.gather_metrics();
        return Ok(Response::builder()
            .status(StatusCode::OK)
            .header("Content-Type", "text/plain; charset=utf-8")
            .body(Body::from(metrics_data))
            .unwrap());
    }

    if path == "/rate-limiter-status" {
        info!(request_id = %request_id, "Rate limiter status request received.");
        return rate_limiter_status_handler(proxy_state.rate_limiters.clone()).await;
    }

    info!(request_id = %request_id, "Handling request for path: {}", path);

    increment_counter_async(proxy_state.metrics.http_requests_total.clone()).await;
    set_gauge_async(proxy_state.metrics.ongoing_requests.clone(), proxy_state.ongoing_requests.load(Ordering::SeqCst) as i64 + 1).await;

    let start_time = Instant::now();

    let static_file_path = format!("./static{}", path);
    if Path::new(&static_file_path).exists() {
        return serve_static_file(&static_file_path).await;
    }

    let (
        target_urls,
        target_url_len,
        auth_config,
        retry_strategy,
        request_transforms,
        response_transforms,
        target_circuit_breaker_config,
        target_timeout,
        target_cache_config,
        logging_config,
        cors_config,
    ) = match find_target(&proxy_state, &path, &original_req, &default_retry_config, default_timeout_seconds) {
        Some(t) => t,
        None => {
            error!(request_id = %request_id, "No target found for path: {}", path);
            increment_counter_async(proxy_state.metrics.failed_requests.clone()).await;
            increment_counter_with_label_async(proxy_state.metrics.error_counts.clone(), "no_target".to_string()).await;
            return Ok(ProxyError::NotFound("No target found for the given path".to_string()).into());
        }
    };

    if let Some(rate_limiter) = proxy_state.rate_limiters.get_limiter(&path).await {
        info!(request_id = %request_id, "Rate limiter found for path: {}", path);
        match rate_limiter.acquire().await {
            Ok(_) => {
                info!(request_id = %request_id, "Token acquired, proceeding with request processing.");
            }
            Err(RateLimitError::Exhausted) => {
                error!(request_id = %request_id, "Rate limit exceeded for path: {}", path);
                return Ok(ProxyError::TooManyRequests("Rate limit exceeded".to_string()).into());
            }
            Err(e) => {
                error!(request_id = %request_id, "Unexpected error: {:?}", e);
                return Ok(ProxyError::InternalServerError("Unexpected error occurred".to_string()).into());
            }
        }
    }

    if let Some(auth_config) = auth_config {
        match auth_config.auth_type {
            AuthenticationType::JWT => {
                if let Some(secret) = &auth_config.jwt_secret {
                    if let Err(err) = validate_jwt(&original_req, secret).await {
                        error!(request_id = %request_id, "JWT validation failed: {}", err);
                        increment_counter_async(proxy_state.metrics.failed_requests.clone()).await;
                        increment_counter_with_label_async(proxy_state.metrics.error_counts.clone(), "auth_error".to_string()).await;
                        return Ok(ProxyError::Unauthorized(err).into());
                    }
                }
            },
            AuthenticationType::Basic => {
                if let Some(users) = &auth_config.basic_users {
                    if let Err(err) = validate_basic(&original_req, users).await {
                        error!(request_id = %request_id, "Basic auth validation failed: {}", err);
                        increment_counter_async(proxy_state.metrics.failed_requests.clone()).await;
                        increment_counter_with_label_async(proxy_state.metrics.error_counts.clone(), "auth_error".to_string()).await;
                        return Ok(ProxyError::Unauthorized(err).into());
                    }
                }
            },
        }
    }

    if let Some(config) = &logging_config {
        if config.log_requests {
            info!(request_id = %request_id, "Incoming request: {:?}", original_req);
        }
    }

    let cache_key = create_cache_key(&target_urls[0], &original_req, None);

    if let Some(cache_config) = &target_cache_config {
        if let Some(cache) = proxy_state.caches.get(&cache_key) {
            if let Some(cache_entry) = cache.get(&cache_key).await {
                info!(request_id = %request_id, "Cache hit for: {}", cache_key);
                let vary_headers = cache_entry.vary_headers.clone();
                let new_cache_key = create_cache_key(&target_urls[0], &original_req, vary_headers.as_ref());
                if let Some(entry) = cache.get(&new_cache_key).await {
                    let response_size = entry.response.len();
                    observe_histogram_async(proxy_state.metrics.response_size_bytes.clone(), response_size as f64).await;
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

                    increment_counter_async(proxy_state.metrics.http_responses_total.clone()).await;
                    increment_counter_async(proxy_state.metrics.successful_requests.clone()).await;
                    observe_histogram_async(proxy_state.metrics.request_duration_seconds.clone(), start_time.elapsed().as_secs_f64()).await;

                    return Ok(response);
                }
            }
        }
    }

    let circuit_breaker_key = format!("{}:{}", target_urls[0], "circuit_breaker");
    let circuit_breaker = {
        let entry = proxy_state.circuit_breakers.entry(circuit_breaker_key.clone()).or_insert_with(|| {
            Arc::new(RwLock::new(CircuitBreaker::new(target_circuit_breaker_config.as_ref().unwrap_or(&*default_circuit_breaker_config))))
        });
        Arc::clone(entry.value())
    };

    {
        let mut circuit_breaker_guard = circuit_breaker.write().await;
        if !circuit_breaker_guard.can_attempt().await {
            error!(request_id = %request_id, "Circuit breaker is open for: {}", target_urls[0]);
            return Ok(ProxyError::CircuitBreakerOpen.into());
        }
    }

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
        let target_url = if let Some(lb) = &proxy_state.load_balancer {
            let lb = lb.write().await;
            lb.get_target(&path, Some(&client_ip.to_string())).await.unwrap_or_else(|| target_urls[0].clone())
        } else {
            target_urls[0].clone()
        };
        

        let mut req = rebuild_request(&mut original_req, &target_url, target_url_len, client_ip).await;
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
                    {
                        let mut circuit_breaker_guard = circuit_breaker.write().await;
                        if circuit_breaker_guard.get_state() == CircuitState::HalfOpen {
                            circuit_breaker_guard.transition_to_closed().await;
                        } else {
                            circuit_breaker_guard.record_success().await;
                        }
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

                        let cors_config = cors_config.or_else(|| proxy_state.default_cors_config.clone());
                        if let Some(cors_config) = cors_config {
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
                    {
                        let mut circuit_breaker_guard = circuit_breaker.write().await;
                        circuit_breaker_guard.record_failure().await;
                    }
                    return Ok(ProxyError::InternalServerError(err.to_string()).into());
                }
            }
            Err(_) => {
                if Instant::now().duration_since(start_time) >= Duration::from_secs(target_timeout) {
                    warn!(request_id = %request_id, "Global timeout exceeded, no more retries");
                    {
                        let mut circuit_breaker_guard = circuit_breaker.write().await;
                        circuit_breaker_guard.record_failure().await;
                    }
                    return Ok(ProxyError::Timeout.into());
                } else {
                    warn!(request_id = %request_id, "Timeout after {} seconds, retrying...", target_timeout);
                    retries += 1;
                    if retries < max_attempts {
                        sleep(retry_strategy.delay()).await;
                        continue;
                    } else {
                        error!(request_id = %request_id, "Timeout after retries");
                        {
                            let mut circuit_breaker_guard = circuit_breaker.write().await;
                            circuit_breaker_guard.record_failure().await;
                        }
                        return Ok(ProxyError::Timeout.into());
                    }
                }
            }
        }
    }

    error!(request_id = %request_id, "Maximum retries exceeded");
    {
        let mut circuit_breaker_guard = circuit_breaker.write().await;
        circuit_breaker_guard.record_failure().await;
        if circuit_breaker_guard.get_state() == CircuitState::HalfOpen {
            circuit_breaker_guard.transition_to_open().await;
        }
    }
    Ok(ProxyError::ServiceUnavailable("Maximum retries exceeded".to_string()).into())
}

fn find_target<'a>(
    proxy_state: &'a ProxyState,
    path: &str,
    req: &Request<Body>,
    default_retry_config: &Arc<RetryConfig>,
    default_timeout_seconds: u64,
) -> Option<(
    Vec<String>,
    usize,
    Option<AuthenticationConfig>,
    Box<dyn RetryStrategy>,
    Option<Vec<Transform>>,
    Option<Vec<Transform>>,
    Option<CircuitBreakerConfig>,
    u64,
    Option<CacheConfig>,
    Option<LoggingConfig>,
    Option<CorsConfig>,
)> {
    let target_map = &proxy_state.target_map;
    let mut target = None;
    for entry in target_map.iter() {
        let (p, (
            urls,
            auth_config,
            retries,
            req_transforms,
            resp_transforms,
            cb_config,
            routing_header,
            routing_values,
            cache_config,
            t_timeout,
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
                            vec![target_url.clone()],
                            p.len(),
                            auth_config.clone(),
                            retries.as_ref().unwrap_or(default_retry_config).to_strategy(),
                            req_transforms.clone(),
                            resp_transforms.clone(),
                            cb_config.clone(),
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
                    urls.clone(),
                    p.len(),
                    auth_config.clone(),
                    retries.as_ref().unwrap_or(default_retry_config).to_strategy(),
                    req_transforms.clone(),
                    resp_transforms.clone(),
                    cb_config.clone(),
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

async fn rebuild_request(
    original_req: &mut Request<Body>,
    target_url: &str,
    target_url_len: usize,
    client_ip: SocketAddr,
) -> Request<Body> {
    let path = original_req.uri().path();
    let query = original_req.uri().query().unwrap_or_default();
    let new_path = &path[target_url_len..];

    let uri_string = if query.is_empty() {
        format!("{}{}", target_url, new_path)
    } else {
        format!("{}{}?{}", target_url, new_path, query)
    };
    let uri = uri_string.parse::<Uri>().expect("Invalid URI");

    let authority = uri.authority().map(|auth| auth.to_string());

    let mut builder = Request::builder()
        .method(original_req.method())
        .uri(uri)
        .version(original_req.version());

    if let Some(headers) = builder.headers_mut() {
        for (key, value) in original_req.headers().iter() {
            headers.insert(key.clone(), value.clone());
        }

        let client_ip = client_ip.ip().to_string();

        let x_forwarded_for = original_req
            .headers()
            .get("X-Forwarded-For")
            .and_then(|header| header.to_str().ok())
            .map(|existing| format!("{}, {}", existing, client_ip))
            .unwrap_or_else(|| client_ip.to_string());

        headers.insert("X-Forwarded-For", HeaderValue::from_str(&x_forwarded_for).unwrap());

        if let Some(auth) = authority {
            headers.insert(HOST, HeaderValue::from_str(&auth).unwrap());
        }

        headers.insert("X-Proxy", HeaderValue::from_static("road7"));
    }

    let body_bytes = hyper::body::to_bytes(original_req.body_mut()).await.unwrap_or_else(|_| hyper::body::Bytes::new());
    let new_body = Body::from(body_bytes);

    builder
        .body(new_body)
        .expect("Failed to rebuild request")
}

fn apply_request_transforms(req: &mut Request<Body>, transforms: &[Transform]) {
    Transform::apply_request_transforms(req, transforms);
}

fn apply_response_transforms(resp: &mut Response<Body>, transforms: &[Transform]) {
    Transform::apply_response_transforms(resp, transforms);
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
        _ => {
            eprintln!("Invalid log level: {}", config.server.max_logging_level);
            std::process::exit(1);
        }
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
