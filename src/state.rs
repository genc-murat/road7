use crate::{
    bot_detector:: BotDetectorConfig, cache::{ Cache, CacheConfig, CorsConfig}, circuit_breaker::{CircuitBreaker, CircuitBreakerConfig}, config::{ AuthenticationConfig, LoggingConfig, Target}, load_balancer::LoadBalancer, metrics::Metrics, rate_limiter::RateLimiterManager, retry::RetryConfig, transform::Transform
};
use hyper::{Body, Response, StatusCode};
use prometheus::{Histogram, IntCounter, IntCounterVec, IntGauge};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::fs::File;
use tokio::io::AsyncReadExt;
use tokio::sync::{RwLock, Semaphore};

use tracing::info;

use dashmap::DashMap;
use std::sync::atomic::AtomicUsize;

pub struct ProxyState {
    pub target_map: DashMap<
        String,
        (
            Vec<String>,
            Option<AuthenticationConfig>,
            Option<RetryConfig>,
            Option<Vec<Transform>>,
            Option<Vec<Transform>>,
            Option<CircuitBreakerConfig>,
            Option<String>,
            Option<HashMap<String, String>>,
            Option<CacheConfig>,
            Option<u64>,
            Option<LoggingConfig>,
            Option<CorsConfig>,
        ),
    >,
    pub circuit_breakers: DashMap<String, Arc<RwLock<CircuitBreaker>>>,
    pub caches: DashMap<String, Cache>,
    pub concurrency_limiter: Arc<Semaphore>,
    pub ongoing_requests: Arc<AtomicUsize>,
    pub metrics: Arc<Metrics>,
    pub default_cors_config: Option<CorsConfig>,
    pub load_balancer: Option<Arc<RwLock<LoadBalancer>>>,
    pub bot_detector: Option<Arc<BotDetectorConfig>>, 
    pub rate_limiters: Arc<RateLimiterManager>,
}

impl ProxyState {
    pub async fn shutdown(&self) {
        self.rate_limiters.shutdown().await;
        info!("Proxy state has been shutdown gracefully.");
    }
}

pub struct ProxyStateBuilder {
    target_map: Option<DashMap<String, (
        Vec<String>,
        Option<AuthenticationConfig>,
        Option<RetryConfig>,
        Option<Vec<Transform>>,
        Option<Vec<Transform>>,
        Option<CircuitBreakerConfig>,
        Option<String>,
        Option<HashMap<String, String>>,
        Option<CacheConfig>,
        Option<u64>,
        Option<LoggingConfig>,
        Option<CorsConfig>,
    )>>,
    circuit_breakers: Option<DashMap<String, Arc<RwLock<CircuitBreaker>>>>,
    caches: Option<DashMap<String, Cache>>,
    concurrency_limiter: Option<Arc<Semaphore>>,
    ongoing_requests: Option<Arc<AtomicUsize>>,
    metrics: Option<Arc<Metrics>>,
    default_cors_config: Option<Option<CorsConfig>>,
    load_balancer: Option<Option<Arc<RwLock<LoadBalancer>>>>,
    bot_detector: Option<Option<Arc<BotDetectorConfig>>>,
    rate_limiters: Option<Arc<RateLimiterManager>>,
}

impl ProxyStateBuilder {
    pub fn new() -> Self {
        Self {
            target_map: None,
            circuit_breakers: None,
            caches: None,
            concurrency_limiter: None,
            ongoing_requests: None,
            metrics: None,
            default_cors_config: None,
            load_balancer: None,
            bot_detector: None,
            rate_limiters: None,
        }
    }

    pub fn with_target_map(mut self, targets: &[Target]) -> Self {
        self.target_map = Some(build_target_map(targets));
        self
    }

    pub fn with_circuit_breakers(mut self, circuit_breakers: DashMap<String, Arc<RwLock<CircuitBreaker>>>) -> Self {
        self.circuit_breakers = Some(circuit_breakers);
        self
    }

    pub fn with_caches(mut self, caches: DashMap<String, Cache>) -> Self {
        self.caches = Some(caches);
        self
    }

    pub fn with_concurrency_limiter(mut self, concurrency_limiter: Arc<Semaphore>) -> Self {
        self.concurrency_limiter = Some(concurrency_limiter);
        self
    }

    pub fn with_ongoing_requests(mut self, ongoing_requests: Arc<AtomicUsize>) -> Self {
        self.ongoing_requests = Some(ongoing_requests);
        self
    }

    pub fn with_metrics(mut self, metrics: Arc<Metrics>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    pub fn with_default_cors_config(mut self, default_cors_config: Option<CorsConfig>) -> Self {
        self.default_cors_config = Some(default_cors_config);
        self
    }

    pub fn with_load_balancer(mut self, load_balancer: Option<Arc<RwLock<LoadBalancer>>>) -> Self {
        self.load_balancer = Some(load_balancer);
        self
    }

    pub fn with_bot_detector(mut self, bot_detector: Option<Arc<BotDetectorConfig>>) -> Self {
        self.bot_detector = Some(bot_detector);
        self
    }

    pub fn with_rate_limiters(mut self, rate_limiters: Arc<RateLimiterManager>) -> Self {
        self.rate_limiters = Some(rate_limiters);
        self
    }

    pub fn build(self) -> Result<ProxyState, &'static str> {
        Ok(ProxyState {
            target_map: self.target_map.ok_or("target_map is required")?,
            circuit_breakers: self.circuit_breakers.ok_or("circuit_breakers are required")?,
            caches: self.caches.ok_or("caches are required")?,
            concurrency_limiter: self.concurrency_limiter.ok_or("concurrency_limiter is required")?,
            ongoing_requests: self.ongoing_requests.ok_or("ongoing_requests are required")?,
            metrics: self.metrics.ok_or("metrics are required")?,
            default_cors_config: self.default_cors_config.ok_or("default_cors_config is required")?,
            load_balancer: self.load_balancer.ok_or("load_balancer is required")?,
            bot_detector: self.bot_detector.ok_or("bot_detector is required")?,
            rate_limiters: self.rate_limiters.ok_or("rate_limiters are required")?,
        })
    }
}

pub(crate) fn build_target_map(
    targets: &[Target],
) -> DashMap<
    String,
    (
        Vec<String>,
        Option<AuthenticationConfig>,
        Option<RetryConfig>,
        Option<Vec<Transform>>,
        Option<Vec<Transform>>,
        Option<CircuitBreakerConfig>,
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
                target.urls.clone(),
                target.authentication.clone(),
                target.retries.clone(),
                target.request_transforms.clone(),
                target.response_transforms.clone(),
                target.circuit_breaker_config.clone(),
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

pub async fn increment_counter_async(counter: IntCounter) {
    tokio::spawn(async move {
        counter.inc();
    }).await.unwrap();
}

pub async fn increment_counter_with_label_async(counter_vec: IntCounterVec, label: String) {
    tokio::spawn(async move {
        counter_vec.with_label_values(&[&label]).inc();
    }).await.unwrap();
}

pub async fn observe_histogram_async(histogram: Histogram, value: f64) {
    tokio::spawn(async move {
        histogram.observe(value);
    }).await.unwrap();
}

pub async fn set_gauge_async(gauge: IntGauge, value: i64) {
    tokio::spawn(async move {
        gauge.set(value);
    }).await.unwrap();
}

pub async fn serve_static_file(path: &str) -> Result<Response<Body>, hyper::Error> {
    match File::open(path).await {
        Ok(mut file) => {
            let mut contents = Vec::new();
            file.read_to_end(&mut contents).await.unwrap();
            Ok(Response::builder()
                .status(StatusCode::OK)
                .body(Body::from(contents))
                .unwrap())
        }
        Err(_) => Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::from("File not found"))
            .unwrap()),
    }
}

pub async fn rate_limiter_status_handler(
    rate_limiters: Arc<RateLimiterManager>,
) -> Result<Response<Body>, hyper::Error> {
    let statuses = rate_limiters.get_all_statuses().await;
    let body = serde_json::to_string(&statuses).unwrap();
    Ok(Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "application/json")
        .body(Body::from(body))
        .unwrap())
}
