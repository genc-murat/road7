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
#[cfg(test)]
mod tests {
    use super::*;
    use hyper::{Body, Request, header::HeaderValue};
    use std::collections::HashMap;

    #[tokio::test]
    async fn test_validate_basic_valid_credentials() {
        let mut valid_users = HashMap::new();
        valid_users.insert("user1".to_string(), "password1".to_string());

        let encoded_cred = base64::encode("user1:password1");
        let auth_value = format!("Basic {}", encoded_cred);

        let req = Request::builder()
            .header(AUTHORIZATION, HeaderValue::from_str(&auth_value).unwrap())
            .body(Body::empty())
            .unwrap();

        let result = validate_basic(&req, &valid_users).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_validate_basic_invalid_credentials() {
        let mut valid_users = HashMap::new();
        valid_users.insert("user1".to_string(), "password1".to_string());

        let encoded_cred = base64::encode("user1:wrongpassword");
        let auth_value = format!("Basic {}", encoded_cred);

        let req = Request::builder()
            .header(AUTHORIZATION, HeaderValue::from_str(&auth_value).unwrap())
            .body(Body::empty())
            .unwrap();

        let result = validate_basic(&req, &valid_users).await;
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), "Invalid credentials");
    }

    #[tokio::test]
    async fn test_validate_basic_missing_header() {
        let valid_users = HashMap::new();

        let req = Request::builder()
            .body(Body::empty())
            .unwrap();

        let result = validate_basic(&req, &valid_users).await;
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), "Authorization header missing");
    }

    #[tokio::test]
    async fn test_validate_basic_invalid_scheme() {
        let valid_users = HashMap::new();

        let encoded_cred = base64::encode("user1:password1");
        let auth_value = format!("Bearer {}", encoded_cred);

        let req = Request::builder()
            .header(AUTHORIZATION, HeaderValue::from_str(&auth_value).unwrap())
            .body(Body::empty())
            .unwrap();

        let result = validate_basic(&req, &valid_users).await;
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), "Invalid authorization scheme");
    }

    #[tokio::test]
    async fn test_validate_basic_invalid_base64() {
        let valid_users = HashMap::new();

        let auth_value = "Basic invalid_base64";

        let req = Request::builder()
            .header(AUTHORIZATION, HeaderValue::from_str(auth_value).unwrap())
            .body(Body::empty())
            .unwrap();

        let result = validate_basic(&req, &valid_users).await;
        assert!(result.is_err());
        assert_eq!(result.err().unwrap(), "Failed to decode base64 credentials");
    }
}
