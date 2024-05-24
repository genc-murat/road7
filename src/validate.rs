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

const MAX_HEADER_SIZE: usize = 8192;
const MAX_BODY_SIZE: usize = 1048576;

pub async fn validate_request(req: &mut Request<Body>) -> Result<(), String> {
    validate_uri(req.uri().to_string())?;
    validate_headers(req.headers())?;
    validate_body(req.body_mut()).await?;
    Ok(())
}

fn validate_uri(uri: String) -> Result<(), String> {
    if !URI_REGEX.is_match(&uri) {
        return Err("Invalid URI detected".to_string());
    }
    Ok(())
}

fn validate_headers(headers: &hyper::HeaderMap) -> Result<(), String> {
    let total_header_size: usize = headers
        .iter()
        .map(|(k, v)| k.as_str().len() + v.as_bytes().len())
        .sum();

    if total_header_size > MAX_HEADER_SIZE {
        return Err("Header size exceeds the limit".to_string());
    }

    for (name, value) in headers {
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

    Ok(())
}

async fn validate_body(body: &mut Body) -> Result<(), String> {
    let body_bytes = to_bytes(body).await.map_err(|_| "Failed to read body".to_string())?;
    if body_bytes.len() > MAX_BODY_SIZE {
        return Err("Body size exceeds the limit".to_string());
    }

    let body_content = String::from_utf8_lossy(&body_bytes);
    let sanitized_body = clean(&body_content);

    if body_content != sanitized_body {
        return Err("HTML sanitization failed: potentially harmful content detected".to_string());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::{Body, Request};
    use hyper::header::{HeaderMap, HeaderValue};

    #[tokio::test]
    async fn test_valid_uri() {
        let req = Request::new(Body::empty());
        assert!(validate_uri(req.uri().to_string()).is_ok());
    }

    #[tokio::test]
    async fn test_invalid_uri() {
        let invalid_uri = "http://example.com/<script>alert('xss')</script>";
        assert!(validate_uri(invalid_uri.to_string()).is_err());
    }

    #[tokio::test]
    async fn test_valid_headers() {
        let mut headers = HeaderMap::new();
        headers.insert("Content-Type", HeaderValue::from_static("application/json"));
        headers.insert("User-Agent", HeaderValue::from_static("Mozilla/5.0"));
        assert!(validate_headers(&headers).is_ok());
    }

    #[tokio::test]
    async fn test_invalid_header_name() {
        let invalid_header_name = "Invalid-Header-Name@";
        let invalid_header_value = "value";

        let result = validate_header(invalid_header_name, invalid_header_value);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), format!("Invalid header name detected: {}", invalid_header_name));
    }

    #[tokio::test]
    async fn test_invalid_header_value() {
        let valid_header_name = "Content-Type";
        let invalid_header_value = "application/<script>json</script>";

        let result = validate_header(valid_header_name, invalid_header_value);
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), format!("Invalid header value detected for {}: {:?}", valid_header_name, invalid_header_value));
    }

    #[tokio::test]
    async fn test_valid_body() {
        let body = Body::from("This is a safe body.");
        let mut req = Request::new(body);
        assert!(validate_body(req.body_mut()).await.is_ok());
    }

    #[tokio::test]
    async fn test_invalid_body() {
        let body = Body::from("<script>alert('xss')</script>");
        let mut req = Request::new(body);
        assert!(validate_body(req.body_mut()).await.is_err());
    }

    #[tokio::test]
    async fn test_exceeds_body_size() {
        let large_body = "a".repeat(MAX_BODY_SIZE + 1);
        let body = Body::from(large_body);
        let mut req = Request::new(body);
        assert!(validate_body(req.body_mut()).await.is_err());
    }

    fn validate_header(name: &str, value: &str) -> Result<(), String> {
        if !HEADER_NAME_REGEX.is_match(name) {
            return Err(format!("Invalid header name detected: {}", name));
        }

        let valid_value = match name {
            "Content-Type" => CONTENT_TYPE_REGEX.is_match(value),
            "User-Agent" => USER_AGENT_REGEX.is_match(value),
            _ => DEFAULT_HEADER_VALUE_REGEX.is_match(value),
        };

        if !valid_value {
            return Err(format!("Invalid header value detected for {}: {:?}", name, value));
        }

        Ok(())
    }
}
