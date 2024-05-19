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
