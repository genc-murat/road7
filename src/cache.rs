use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use std::time::Instant;

use dashmap::DashMap;
use hyper::{Body, Response, StatusCode, Request};
use hyper::header::{HeaderMap, HeaderName, HeaderValue, VARY};
use serde::{Deserialize, Serialize};
use tokio::sync::RwLock;
use tokio::time::{Duration, sleep};
use tracing::{info, warn};
use uuid::Uuid;
use http::header::{ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS, ACCESS_CONTROL_ALLOW_ORIGIN};

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CorsConfig {
    pub enabled: bool,
    #[serde(rename = "Access-Control-Allow-Origin")]
    pub allow_origin: Option<String>,
    #[serde(rename = "Access-Control-Allow-Headers")]
    pub allow_headers: Option<String>,
    #[serde(rename = "Access-Control-Allow-Methods")]
    pub allow_methods: Option<String>,
}

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct CacheConfig {
    pub ttl_seconds: u64,
    pub max_size: usize,
    #[serde(default = "default_serialize")]
    pub serialize: bool,
    #[serde(default = "default_cleanup_interval")]
    pub cleanup_interval_seconds: u64,
}

fn default_cleanup_interval() -> u64 {
    60
}


fn default_serialize() -> bool {
    false
}

#[derive(Debug, Clone)]
pub struct Cache {
    pub entries: Arc<DashMap<String, CacheEntry>>,
    pub ttl: Duration,
    pub serialize: bool,
}

#[derive(Debug, Clone)]
pub struct CacheEntry {
    pub response: Vec<u8>,
    pub headers: HeaderMap,
    pub status: StatusCode,
    pub expires_at: Instant,
    pub cors_headers: Option<CorsHeaders>,
    pub etag: Option<String>,
    pub last_modified: Option<String>,
    pub vary_headers: Option<Vec<String>>,
}

#[derive(Debug, Clone)]
pub struct CorsHeaders {
    pub allow_origin: HeaderValue,
    pub allow_headers: HeaderValue,
    pub allow_methods: HeaderValue,
}

impl CacheEntry {
    pub async fn from_response(response: &mut Response<Body>, cors_config: &Option<CorsConfig>) -> Self {
        let headers = response.headers().clone();
        let status = response.status();
        let cors_headers = cors_config.as_ref().map(|config| {
            CorsHeaders {
                allow_origin: HeaderValue::from_str(config.allow_origin.as_deref().unwrap_or("*")).unwrap(),
                allow_headers: HeaderValue::from_str(config.allow_headers.as_deref().unwrap_or("*")).unwrap(),
                allow_methods: HeaderValue::from_str(config.allow_methods.as_deref().unwrap_or("GET,POST,PUT,DELETE,OPTIONS,PATCH")).unwrap(),
            }
        });

        let body_bytes = hyper::body::to_bytes(response.body_mut()).await.unwrap_or_else(|_| hyper::body::Bytes::new());

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
    pub fn new(config: &CacheConfig) -> Self {
        let cache = Self {
            entries: Arc::new(DashMap::new()),
            ttl: Duration::from_secs(config.ttl_seconds),
            serialize: config.serialize,
        };

        let cache_clone = cache.clone();
        let cleanup_interval = Duration::from_secs(config.cleanup_interval_seconds);
        tokio::spawn(async move {
            loop {
                cache_clone.clean_expired_entries().await;
                sleep(cleanup_interval).await;
            }
        });

        cache
    }

    pub async fn get(&self, key: &str) -> Option<CacheEntry> {
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

    pub async fn put(&self, key: String, response: CacheEntry) {
        let entry = CacheEntry {
            response: if self.serialize {
                match serde_json::to_vec(&response.response) {
                    Ok(serialized) => serialized,
                    Err(err) => {
                        warn!("Failed to serialize cache entry: {}", err);
                        response.response.clone()
                    }
                }
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

    pub async fn remove_expired(&self, key: &str) {
        self.entries.remove(key);
        warn!("Cache entry removed for key: {}", key);
    }

    pub async fn clean_expired_entries(&self) {
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

pub fn create_cache_key(target_url: &str, req: &Request<Body>, vary_headers: Option<&Vec<String>>) -> String {
    let mut hasher = DefaultHasher::new();
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
