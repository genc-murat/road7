mod auth {
    pub mod jwt;
    pub mod basic;
}

mod retry;
mod rate_limiter;
mod transform;
mod error;
mod circuit_breaker;

use crate::auth::jwt::validate_jwt;
use crate::auth::basic::validate_basic;
use circuit_breaker::{CircuitBreaker, CircuitBreakerConfig, CircuitState};
use error::ProxyError;
use retry::{RetryConfig, RetryStrategy};
use rate_limiter::{RateLimiter, RateLimiterConfig};
use transform::Transform;
use hyper::{Body, Client, Request, Response, Server, StatusCode, Uri};
use hyper::service::{make_service_fn, service_fn};
use hyper::header::{HeaderValue, STRICT_TRANSPORT_SECURITY, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS, CONTENT_SECURITY_POLICY, X_XSS_PROTECTION, REFERRER_POLICY, ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, USER_AGENT, HOST, VARY};
use hyper::client::HttpConnector;
use hyper_tls::HttpsConnector;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{RwLock, Semaphore};
use serde::{Deserialize, Serialize};
use tokio::time::{sleep, Duration, timeout};
use tracing::{error, info, warn};
use tracing_subscriber;
use std::convert::Infallible;
use tokio::signal;
use std::time::Instant;
use std::net::SocketAddr;
use tracing_appender::non_blocking;
use std::hash::{Hash, Hasher};
use dashmap::DashMap;
use http::header::{HeaderMap, HeaderName};
use regex::Regex;
use ammonia::clean;
use hyper::body::to_bytes;
use uuid::Uuid;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::runtime::Builder;

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
    authentication: Option<AuthenticationConfig>,
    retries: Option<RetryConfig>,
    request_transforms: Option<Vec<Transform>>,
    response_transforms: Option<Vec<Transform>>,
    circuit_breaker_config: Option<CircuitBreakerConfig>,
    rate_limiter_config: Option<RateLimiterConfig>,
    routing_header: Option<String>,
    routing_values: Option<HashMap<String, String>>,
    timeout_seconds: Option<u64>,
    cache_config: Option<CacheConfig>,
    logging_config: Option<LoggingConfig>,
    cors_config: Option<CorsConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
struct AuthenticationConfig {
    #[serde(rename = "type")]
    auth_type: AuthenticationType,
    jwt_secret: Option<String>,
    basic_users: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
enum AuthenticationType {
    JWT,
    Basic,
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
    vary_headers: Option<Vec<String>>,
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

        let body_bytes = to_bytes(response.body_mut()).await.unwrap_or_else(|_| hyper::body::Bytes::new());

        let etag = headers.get("ETag").map(|v| v.to_str().unwrap().to_string()).or_else(|| {
            let hash = format!("{:x}", md5::compute(&body_bytes));
            Some(format!("\"{}\"", hash))
        });

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
            vary_headers,
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

        let cache_clone = cache.clone();
        tokio::spawn(async move {
            loop {
                cache_clone.clean_expired_entries().await;
                sleep(Duration::from_secs(60)).await;
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
            vary_headers: response.vary_headers.clone(),
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

    // Limit the size of headers
    let max_header_size = 8192; // 8 KB
    let total_header_size: usize = req.headers().iter().map(|(k, v)| k.as_str().len() + v.as_bytes().len()).sum();
    if total_header_size > max_header_size {
        return Err("Header size exceeds the limit".to_string());
    }

    for (name, value) in req.headers() {
        let header_name_regex = Regex::new(r"^[a-zA-Z0-9\-]+$").unwrap();
        if !header_name_regex.is_match(name.as_str()) {
            return Err(format!("Invalid header name detected: {}", name));
        }

        // Different regex for different headers
        let header_value = value.to_str().unwrap_or_default();
        let valid_value = match name.as_str() {
            "Content-Type" => Regex::new(r"^[a-zA-Z0-9\-._+\/;=\s]+$").unwrap().is_match(header_value),
            "User-Agent" => Regex::new(r"^[a-zA-Z0-9\-._~!$&'()*+,;=\/: ]+$").unwrap().is_match(header_value),
            _ => Regex::new(r"^[\x20-\x7E]+$").unwrap().is_match(header_value), // Default to printable ASCII characters
        };

        if !valid_value {
            return Err(format!("Invalid header value detected for {}: {:?}", name, value));
        }
    }

    let body_bytes = to_bytes(req.body_mut()).await.unwrap();
    let body_content = String::from_utf8_lossy(&body_bytes);
    let sanitized_body = clean(&body_content);

    // Limit the size of the body
    let max_body_size = 1048576; // 1 MB
    if body_bytes.len() > max_body_size {
        return Err("Body size exceeds the limit".to_string());
    }

    if body_content != sanitized_body {
        return Err("HTML sanitization failed: potentially harmful content detected".to_string());
    }

    Ok(())
}

async fn apply_security_headers(headers: &mut HeaderMap, security_headers_config: &Option<SecurityHeadersConfig>) {
    if let Some(config) = security_headers_config.as_ref() {
        insert_header(headers, STRICT_TRANSPORT_SECURITY, &config.strict_transport_security);
        insert_header(headers, X_CONTENT_TYPE_OPTIONS, &config.x_content_type_options);
        insert_header(headers, X_FRAME_OPTIONS, &config.x_frame_options);
        
        if let Some(csp) = &config.content_security_policy {
            headers.insert(CONTENT_SECURITY_POLICY, HeaderValue::from_str(csp).unwrap());
        } else {
            let default_csp = "default-src 'self'; script-src 'self'; object-src 'none'; style-src 'self'; img-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'";
            headers.insert(CONTENT_SECURITY_POLICY, HeaderValue::from_str(default_csp).unwrap());
        }
        
        if let Some(xss_protection) = &config.x_xss_protection {
            headers.insert(X_XSS_PROTECTION, HeaderValue::from_str(xss_protection).unwrap());
        } else {
            headers.insert(X_XSS_PROTECTION, HeaderValue::from_static("1; mode=block"));
        }
        
        if let Some(referrer_policy) = &config.referrer_policy {
            headers.insert(REFERRER_POLICY, HeaderValue::from_str(referrer_policy).unwrap());
        } else {
            headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
        }
        
        insert_header(headers, PERMISSIONS_POLICY, &config.permissions_policy);
        insert_header(headers, FEATURE_POLICY, &config.feature_policy);
        
        info!("Applied security headers: {:?}", headers);
    }
}

fn insert_header(headers: &mut HeaderMap, header_name: HeaderName, header_value: &Option<String>) {
    if let Some(value) = header_value {
        headers.insert(header_name, HeaderValue::from_str(value).unwrap());
    }
}

struct ProxyState {
    target_map: DashMap<
        String,
        (
            String,
            Option<AuthenticationConfig>,
            Option<RetryConfig>,
            Option<Vec<Transform>>,
            Option<Vec<Transform>>,
            Option<CircuitBreakerConfig>,
            Option<RateLimiterConfig>,
            Option<String>,
            Option<HashMap<String, String>>,
            Option<CacheConfig>,
            Option<u64>,
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
        .pool_idle_timeout(Duration::from_secs(30))
        .build(https_connector);

    let initial_target_map = build_target_map(&config.targets);

    let caches = initial_target_map.iter()
        .filter_map(|entry| {
            let (path, (_, _, _, _, _, _, _, _, _, cache_config, _, _, _)) = entry.pair();
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
        Option<AuthenticationConfig>,
        Option<RetryConfig>,
        Option<Vec<Transform>>,
        Option<Vec<Transform>>,
        Option<CircuitBreakerConfig>,
        Option<RateLimiterConfig>,
        Option<String>,
        Option<HashMap<String, String>>,
        Option<CacheConfig>,
        Option<u64>,
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
                target.authentication.clone(),
                target.retries.clone(),
                target.request_transforms.clone(),
                target.response_transforms.clone(),
                target.circuit_breaker_config.clone(),
                target.rate_limiter_config.clone(),
                target.routing_header.clone(),
                target.routing_values.clone(),
                target.cache_config.clone(),
                target.timeout_seconds,
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
        auth_config,
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

    // Kimlik doğrulama kontrolü
    if let Some(auth_config) = auth_config {
        match auth_config.auth_type {
            AuthenticationType::JWT => {
                if let Some(secret) = &auth_config.jwt_secret {
                    if let Err(err) = validate_jwt(&original_req, secret).await {
                        error!(request_id = %request_id, "JWT validation failed: {}", err);
                        return Ok(ProxyError::Unauthorized(err).into());
                    }
                }
            },
            AuthenticationType::Basic => {
                if let Some(users) = &auth_config.basic_users {
                    if let Err(err) = validate_basic(&original_req, users).await {
                        error!(request_id = %request_id, "Basic auth validation failed: {}", err);
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
            RateLimiterConfig::SlidingWindow { header_key, .. } |
            RateLimiterConfig::Quota { header_key, .. } |
            RateLimiterConfig::Dynamic { header_key, .. } => {
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
        let mut req = rebuild_request(&mut original_req, &target_url, target_url_len).await;
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
    Option<AuthenticationConfig>,
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
            auth_config,
            retries,
            req_transforms,
            resp_transforms,
            cb_config,
            rate_limiter_config,
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
                            target_url.clone(),
                            p.len(),
                            auth_config.clone(),
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
                    auth_config.clone(),
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

async fn rebuild_request(
    original_req: &mut Request<Body>,
    target_url: &str,
    target_url_len: usize,
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
        // Forward only the necessary headers
        for (key, value) in original_req.headers().iter() {
            if key != HOST && key != USER_AGENT && key != "X-Forwarded-For" {
                headers.insert(key.clone(), value.clone());
            }
        }
        
        if let Some(auth) = authority {
            headers.insert(HOST, HeaderValue::from_str(&auth).unwrap());
        }
        
        // Add a custom header to indicate the request passed through the proxy
        headers.insert("X-Proxy", HeaderValue::from_static("road7"));
    }
    
    // Rebuild the request body
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
