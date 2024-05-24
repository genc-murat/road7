use serde::{Deserialize, Serialize};
use config::{Config, ConfigError};
use std::collections::HashMap;

use crate::{bot_detector::BotDetectorConfig, cache::{CacheConfig, CorsConfig}, circuit_breaker::CircuitBreakerConfig, load_balancer::LoadBalancerConfig, rate_limiter::RateLimiterConfig, retry::RetryConfig, security_headers::SecurityHeadersConfig, transform::Transform};

pub const DEFAULT_TIMEOUT_SECONDS: u64 = 30;
pub const DEFAULT_POOL_SIZE: usize = 10;
pub const DEFAULT_WORKER_THREADS: usize = 1;

#[derive(Debug, Deserialize, Serialize)]
pub struct ProxyConfig {
    pub server: ServerConfig,
    pub runtime: RuntimeConfig,
    pub targets: Vec<Target>,
    pub retries: RetryConfig,
    #[serde(default)]
    pub default_circuit_breaker_config: CircuitBreakerConfig,
    #[serde(default = "default_timeout_seconds")]
    pub default_timeout_seconds: u64,
    #[serde(default)]
    pub default_rate_limiter_config: Option<RateLimiterConfig>,
    #[serde(default)]
    pub security_headers_config: Option<SecurityHeadersConfig>,
    #[serde(default)]
    pub default_cors_config: Option<CorsConfig>,
    pub load_balancer: Option<LoadBalancerConfig>,
    pub bot_detector: Option<BotDetectorConfig>,
}

fn default_timeout_seconds() -> u64 {
    DEFAULT_TIMEOUT_SECONDS
}

fn default_pool_size() -> usize {
    DEFAULT_POOL_SIZE
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub max_logging_level: String,
    #[serde(default = "default_pool_size")]
    pub pool_size: usize,
    pub recv_buffer_size: Option<usize>,
    pub send_buffer_size: Option<usize>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct RuntimeConfig {
    #[serde(default = "default_worker_threads")]
    pub worker_threads: usize,
}

fn default_worker_threads() -> usize {
    DEFAULT_WORKER_THREADS
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Target {
    pub path: String,
    pub urls: Vec<String>,
    pub authentication: Option<AuthenticationConfig>,
    pub retries: Option<RetryConfig>,
    pub request_transforms: Option<Vec<Transform>>,
    pub response_transforms: Option<Vec<Transform>>,
    pub circuit_breaker_config: Option<CircuitBreakerConfig>,
    pub rate_limiter_config: Option<RateLimiterConfig>,
    pub routing_header: Option<String>,
    pub routing_values: Option<HashMap<String, String>>,
    pub timeout_seconds: Option<u64>,
    pub cache_config: Option<CacheConfig>,
    pub logging_config: Option<LoggingConfig>,
    pub cors_config: Option<CorsConfig>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct AuthenticationConfig {
    #[serde(rename = "type")]
    pub auth_type: AuthenticationType,
    pub jwt_secret: Option<String>,
    pub basic_users: Option<HashMap<String, String>>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub enum AuthenticationType {
    JWT,
    Basic,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct LoggingConfig {
    pub log_requests: bool,
    pub log_responses: bool,
}

pub fn read_config() -> Result<ProxyConfig, ConfigError> {
    let mut settings = Config::default();
    settings.merge(config::File::with_name("config"))?;
    settings.try_into()
}
