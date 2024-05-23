use hyper::{Body, Request};
use hyper::body::to_bytes;
use regex::Regex;
use ammonia::clean;

pub async fn validate_request(req: &mut Request<Body>) -> Result<(), String> {
    let uri = req.uri().to_string();
    let uri_regex = Regex::new(r"^[a-zA-Z0-9\-._~:/?#\[\]@!$&'()*+,;=%]+$").unwrap();
    if !uri_regex.is_match(&uri) {
        return Err("Invalid URI detected".to_string());
    }

    let max_header_size = 8192;
    let total_header_size: usize = req.headers().iter().map(|(k, v)| k.as_str().len() + v.as_bytes().len()).sum();
    if total_header_size > max_header_size {
        return Err("Header size exceeds the limit".to_string());
    }

    for (name, value) in req.headers() {
        let header_name_regex = Regex::new(r"^[a-zA-Z0-9\-]+$").unwrap();
        if !header_name_regex.is_match(name.as_str()) {
            return Err(format!("Invalid header name detected: {}", name));
        }

        let header_value = value.to_str().unwrap_or_default();
        let valid_value = match name.as_str() {
            "Content-Type" => Regex::new(r"^[a-zA-Z0-9\-._+\/;=\s]+$").unwrap().is_match(header_value),
            "User-Agent" => Regex::new(r"^[a-zA-Z0-9\-._~!$&'()*+,;=\/: ]+$").unwrap().is_match(header_value),
            _ => Regex::new(r"^[\x20-\x7E]+$").unwrap().is_match(header_value), 
        };

        if !valid_value {
            return Err(format!("Invalid header value detected for {}: {:?}", name, value));
        }
    }

    let body_bytes = to_bytes(req.body_mut()).await.unwrap();
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
