use hyper::{Request, Body};
use hyper::header::AUTHORIZATION;
use base64::decode;
use std::collections::HashMap;

pub async fn validate_basic(req: &Request<Body>, valid_users: &HashMap<String, String>) -> Result<(), String> {
    let header_value = match req.headers().get(AUTHORIZATION) {
        Some(value) => value.to_str().map_err(|_| "Invalid header format")?,
        None => return Err("Authorization header missing".to_string()),
    };

    if !header_value.starts_with("Basic ") {
        return Err("Invalid authorization scheme".to_string());
    }

    let encoded_cred = &header_value[6..];
    let decoded_cred = decode(encoded_cred).map_err(|_| "Failed to decode base64 credentials")?;
    let cred_str = String::from_utf8(decoded_cred).map_err(|_| "Invalid UTF-8 sequence")?;

    let mut parts = cred_str.splitn(2, ':');
    let username = parts.next().ok_or("Missing username")?;
    let password = parts.next().ok_or("Missing password")?;

    match valid_users.get(username) {
        Some(valid_password) if valid_password == password => Ok(()),
        _ => Err("Invalid credentials".to_string()),
    }
}
