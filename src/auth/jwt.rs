use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};
use serde::{Deserialize, Serialize};
use hyper::{Request, Body};
use hyper::header::AUTHORIZATION;

#[derive(Debug, Deserialize, Serialize)]
pub struct Claims {
    sub: String,
    company: String,
    exp: usize,
}

pub async fn validate_jwt(req: &Request<Body>, secret: &str) -> Result<Claims, String> {
    let token = match req.headers().get(AUTHORIZATION) {
        Some(value) => {
            let token_str = value.to_str().map_err(|_| "Invalid token format")?;
            token_str.trim_start_matches("Bearer ").to_string()
        },
        None => return Err("Authorization header missing".to_string()),
    };

    let decoding_key = DecodingKey::from_secret(secret.as_ref());
    let validation = Validation::new(Algorithm::HS256);

    decode::<Claims>(&token, &decoding_key, &validation)
        .map(|token_data| token_data.claims)
        .map_err(|err| err.to_string())
}
#[cfg(test)]
mod tests {
    use super::*;
    use hyper::Body;
    use hyper::header::HeaderValue;
    use hyper::Request;
    use jsonwebtoken::{encode, Header, EncodingKey};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn generate_jwt(secret: &str) -> String {
        let my_claims = Claims {
            sub: "1234567890".to_string(),
            company: "ACME".to_string(),
            exp: (SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + 600) as usize,
        };
        encode(&Header::default(), &my_claims, &EncodingKey::from_secret(secret.as_ref())).unwrap()
    }

    #[tokio::test]
    async fn test_validate_jwt_success() {
        let secret = "my_secret";
        let token = generate_jwt(secret);
        let mut req = Request::new(Body::empty());
        req.headers_mut().insert(AUTHORIZATION, HeaderValue::from_str(&format!("Bearer {}", token)).unwrap());

        let result = validate_jwt(&req, secret).await;
        assert!(result.is_ok());
        let claims = result.unwrap();
        assert_eq!(claims.sub, "1234567890");
        assert_eq!(claims.company, "ACME");
    }

    #[tokio::test]
    async fn test_validate_jwt_missing_header() {
        let secret = "my_secret";
        let req = Request::new(Body::empty());

        let result = validate_jwt(&req, secret).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), "Authorization header missing");
    }

    #[tokio::test]
    async fn test_validate_jwt_invalid_token() {
        let secret = "my_secret";
        let mut req = Request::new(Body::empty());
        req.headers_mut().insert(AUTHORIZATION, HeaderValue::from_str("Bearer InvalidToken").unwrap());

        let result = validate_jwt(&req, secret).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("InvalidToken"));
    }
}
