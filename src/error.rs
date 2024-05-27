use hyper::{Body, Response, StatusCode};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ProxyError {
    #[error("Service Unavailable: Circuit Breaker is Open")]
    CircuitBreakerOpen,
    #[error("Too Many Requests: Rate limit exceeded")]
    TooManyRequests(String),
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
    #[error("Forbidden: {0}")]
    Forbidden(String),
}

impl From<ProxyError> for Response<Body> {
    fn from(error: ProxyError) -> Self {
        match error {
            ProxyError::CircuitBreakerOpen => Response::builder()
                .status(StatusCode::SERVICE_UNAVAILABLE)
                .body(Body::from(error.to_string()))
                .unwrap(),
            ProxyError::TooManyRequests(msg) => Response::builder()
                .status(StatusCode::TOO_MANY_REQUESTS)
                .body(Body::from(msg))
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
            ProxyError::Forbidden(_) => Response::builder()
                .status(StatusCode::FORBIDDEN)
                .body(Body::from(error.to_string()))
                .unwrap(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::body::to_bytes;
    use hyper::{Body, StatusCode};
    use tokio::runtime::Runtime;

    #[test]
    fn test_circuit_breaker_open() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let error = ProxyError::CircuitBreakerOpen;
            let response: Response<Body> = error.into();
            
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            let body = to_bytes(response.into_body()).await.unwrap();
            assert_eq!(body, "Service Unavailable: Circuit Breaker is Open".as_bytes());
        });
    }

    #[test]
    fn test_too_many_requests() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let msg = "Rate limit exceeded".to_string();
            let error = ProxyError::TooManyRequests(msg.clone());
            let response: Response<Body> = error.into();
            
            assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
            let body = to_bytes(response.into_body()).await.unwrap();
            assert_eq!(body, msg.as_bytes());
        });
    }

    #[test]
    fn test_timeout() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let error = ProxyError::Timeout;
            let response: Response<Body> = error.into();
            
            assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
            let body = to_bytes(response.into_body()).await.unwrap();
            assert_eq!(body, "Gateway Timeout".as_bytes());
        });
    }

    #[test]
    fn test_internal_server_error() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let msg = "Internal error occurred".to_string();
            let error = ProxyError::InternalServerError(msg.clone());
            let response: Response<Body> = error.into();
            
            assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
            let body = to_bytes(response.into_body()).await.unwrap();
            assert_eq!(body, format!("Internal Server Error: {}", msg).as_bytes());
        });
    }

    #[test]
    fn test_bad_request() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let msg = "Invalid request".to_string();
            let error = ProxyError::BadRequest(msg.clone());
            let response: Response<Body> = error.into();
            
            assert_eq!(response.status(), StatusCode::BAD_REQUEST);
            let body = to_bytes(response.into_body()).await.unwrap();
            assert_eq!(body, format!("Bad Request: {}", msg).as_bytes());
        });
    }

    #[test]
    fn test_not_found() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let msg = "Resource not found".to_string();
            let error = ProxyError::NotFound(msg.clone());
            let response: Response<Body> = error.into();
            
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            let body = to_bytes(response.into_body()).await.unwrap();
            assert_eq!(body, format!("Not Found: {}", msg).as_bytes());
        });
    }

    #[test]
    fn test_service_unavailable() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let msg = "Service temporarily unavailable".to_string();
            let error = ProxyError::ServiceUnavailable(msg.clone());
            let response: Response<Body> = error.into();
            
            assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
            let body = to_bytes(response.into_body()).await.unwrap();
            assert_eq!(body, format!("Service Unavailable: {}", msg).as_bytes());
        });
    }

    #[test]
    fn test_unauthorized() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let msg = "Unauthorized access".to_string();
            let error = ProxyError::Unauthorized(msg.clone());
            let response: Response<Body> = error.into();
            
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
            let body = to_bytes(response.into_body()).await.unwrap();
            assert_eq!(body, format!("Unauthorized: {}", msg).as_bytes());
        });
    }

    #[test]
    fn test_forbidden() {
        let rt = Runtime::new().unwrap();
        rt.block_on(async {
            let msg = "Forbidden resource".to_string();
            let error = ProxyError::Forbidden(msg.clone());
            let response: Response<Body> = error.into();
            
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            let body = to_bytes(response.into_body()).await.unwrap();
            assert_eq!(body, format!("Forbidden: {}", msg).as_bytes());
        });
    }
}
