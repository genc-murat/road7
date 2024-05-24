use hyper::{Body, Request};
use hyper::body::to_bytes;
use regex::Regex;
use ammonia::clean;
use std::collections::HashMap;
use once_cell::sync::Lazy;

static URI_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z0-9\-._~:/?#\[\]@!$&'()*+,;=%]+$").unwrap());
static HEADER_NAME_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z0-9\-]+$").unwrap());
static CONTENT_TYPE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z0-9\-._+\/;=\s]+$").unwrap());
static USER_AGENT_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[a-zA-Z0-9\-._~!$&'()*+,;=\/: ]+$").unwrap());
static DEFAULT_HEADER_VALUE_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r"^[\x20-\x7E]+$").unwrap());

pub async fn validate_request(req: &mut Request<Body>) -> Result<(), String> {
    let uri = req.uri().to_string();
    if !URI_REGEX.is_match(&uri) {
        return Err("Invalid URI detected".to_string());
    }

    let max_header_size = 8192;
    let total_header_size: usize = req.headers()
        .iter()
        .map(|(k, v)| k.as_str().len() + v.as_bytes().len())
        .sum();
    if total_header_size > max_header_size {
        return Err("Header size exceeds the limit".to_string());
    }

    for (name, value) in req.headers() {
        if !HEADER_NAME_REGEX.is_match(name.as_str()) {
            return Err(format!("Invalid header name detected: {}", name));
        }

        let header_value = value.to_str().map_err(|_| format!("Non-ASCII header value detected for {}", name))?;
        let valid_value = match name.as_str() {
            "Content-Type" => CONTENT_TYPE_REGEX.is_match(header_value),
            "User-Agent" => USER_AGENT_REGEX.is_match(header_value),
            _ => DEFAULT_HEADER_VALUE_REGEX.is_match(header_value),
        };

        if !valid_value {
            return Err(format!("Invalid header value detected for {}: {:?}", name, value));
        }
    }

    let body_bytes = to_bytes(req.body_mut()).await.map_err(|_| "Failed to read body".to_string())?;
    let body_content = String::from_utf8_lossy(&body_bytes);
    let sanitized_body = clean(&body_content);

    let max_body_size = 1048576; 
    if body_bytes.len() > max_body_size {
        return Err("Body size exceeds the limit".to_string());
    }

    if body_content != sanitized_body {
        return Err("HTML sanitization failed: potentially harmful content detected".to_string());
    }

    Ok(())
}