use hyper::{Body, Response, StatusCode};
use thiserror::Error;
#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("Service Unavailable: Circuit Breaker is Open")]
    CircuitBreakerOpen,
    #[error("Too Many Requests: Rate limit exceeded")]
    RateLimitExceeded,
    #[error("Gateway Timeout")]
    Timeout,
    #[error("Internal Server Error: {0}")]
    InternalServerError(String),
    #[error("Bad Request: {0}")]
    BadRequest(String),
    #[error("Not Found: {0}")]
    NotFound(String),
    #[error("Service Unavailable: {0}")]
    ServiceUnavailable(String),
    #[error("Unauthorized: {0}")]
    Unauthorized(String),
}

impl From<ProxyError> for Response<Body> {
    fn from(error: ProxyError) -> Self {
        match error {
            ProxyError::CircuitBreakerOpen => Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body(Body::from(error.to_string()))
                .unwrap(),
            ProxyError::RateLimitExceeded => Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .body(Body::from(error.to_string()))
                .unwrap(),
            ProxyError::Timeout => Response::builder()
                .status(StatusCode::GATEWAY_TIMEOUT)
                .body(Body::from(error.to_string()))
                .unwrap(),
            ProxyError::InternalServerError(_) => Response::builder()
                .status(StatusCode::INTERNAL_SERVER_ERROR)
                .body(Body::from(error.to_string()))
                .unwrap(),
            ProxyError::BadRequest(_) => Response::builder()
                .status(StatusCode::BAD_REQUEST)
                .body(Body::from(error.to_string()))
                .unwrap(),
            ProxyError::NotFound(_) => Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::from(error.to_string()))
                .unwrap(),
            ProxyError::ServiceUnavailable(_) => Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body(Body::from(error.to_string()))
                .unwrap(),
            ProxyError::Unauthorized(_) => Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(Body::from(error.to_string()))
                .unwrap(),
        }
    }
}
