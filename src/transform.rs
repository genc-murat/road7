use serde::{Deserialize, Serialize};
use hyper::header::{HeaderMap, HeaderName, HeaderValue};
use tracing::warn;

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct Transform {
    #[serde(rename = "type")]
    pub transform_type: String,
    pub name: String,
    #[serde(default)]
    pub value: Option<String>,
    pub operation: String,
}

impl Transform {
    pub fn apply_request_transforms(req: &mut hyper::Request<hyper::Body>, transforms: &[Transform]) {
        let headers = req.headers_mut();
        Self::apply_header_transforms(headers, transforms);
    }

    pub fn apply_response_transforms(resp: &mut hyper::Response<hyper::Body>, transforms: &[Transform]) {
        let headers = resp.headers_mut();
        Self::apply_header_transforms(headers, transforms);
    }

    fn apply_header_transforms(headers: &mut HeaderMap, transforms: &[Transform]) {
        for transform in transforms {
            if let Ok(header_name) = transform.name.parse::<HeaderName>() {
                match transform.operation.as_str() {
                    "Set" => {
                        let header_value = transform.value.as_deref().unwrap_or("");
                        if let Ok(value) = HeaderValue::from_str(header_value) {
                            headers.insert(header_name, value);
                        } else {
                            headers.insert(header_name, HeaderValue::from_static(""));
                        }
                    },
                    "Remove" => {
                        headers.remove(header_name);
                    },
                    "Append" => {
                        let new_value = headers.get(&header_name)
                            .and_then(|current_value| current_value.to_str().ok())
                            .map_or_else(|| transform.value.as_deref().unwrap_or("").to_string(), |current| {
                                format!("{}{}", current, transform.value.as_deref().unwrap_or(""))
                            });

                        if let Ok(value) = HeaderValue::from_str(&new_value) {
                            headers.insert(header_name, value);
                        } else {
                            headers.insert(header_name, HeaderValue::from_static(""));
                        }
                    },
                    _ => warn!("Unknown transform operation: {}", transform.operation),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::{Request, Response, Body, HeaderMap};

    fn create_transform(name: &str, operation: &str, value: Option<&str>) -> Transform {
        Transform {
            transform_type: "Header".to_string(),
            name: name.to_string(),
            value: value.map(|v| v.to_string()),
            operation: operation.to_string(),
        }
    }

    #[tokio::test]
    async fn test_apply_request_transforms_set() {
        let mut req = Request::builder()
            .uri("http://example.com")
            .body(Body::empty())
            .unwrap();

        let transforms = vec![create_transform("X-Custom-Header", "Set", Some("Value"))];
        Transform::apply_request_transforms(&mut req, &transforms);

        assert_eq!(req.headers().get("X-Custom-Header").unwrap(), "Value");
    }

    #[tokio::test]
    async fn test_apply_request_transforms_remove() {
        let mut req = Request::builder()
            .uri("http://example.com")
            .header("X-Custom-Header", "Value")
            .body(Body::empty())
            .unwrap();

        let transforms = vec![create_transform("X-Custom-Header", "Remove", None)];
        Transform::apply_request_transforms(&mut req, &transforms);

        assert!(req.headers().get("X-Custom-Header").is_none());
    }

    #[tokio::test]
    async fn test_apply_request_transforms_append() {
        let mut req = Request::builder()
            .uri("http://example.com")
            .header("X-Custom-Header", "Value")
            .body(Body::empty())
            .unwrap();

        let transforms = vec![create_transform("X-Custom-Header", "Append", Some("Appended"))];
        Transform::apply_request_transforms(&mut req, &transforms);

        assert_eq!(req.headers().get("X-Custom-Header").unwrap(), "ValueAppended");
    }

    #[tokio::test]
    async fn test_apply_response_transforms_set() {
        let mut resp = Response::builder()
            .body(Body::empty())
            .unwrap();

        let transforms = vec![create_transform("X-Custom-Header", "Set", Some("Value"))];
        Transform::apply_response_transforms(&mut resp, &transforms);

        assert_eq!(resp.headers().get("X-Custom-Header").unwrap(), "Value");
    }

    #[tokio::test]
    async fn test_apply_response_transforms_remove() {
        let mut resp = Response::builder()
            .header("X-Custom-Header", "Value")
            .body(Body::empty())
            .unwrap();

        let transforms = vec![create_transform("X-Custom-Header", "Remove", None)];
        Transform::apply_response_transforms(&mut resp, &transforms);

        assert!(resp.headers().get("X-Custom-Header").is_none());
    }

    #[tokio::test]
    async fn test_apply_response_transforms_append() {
        let mut resp = Response::builder()
            .header("X-Custom-Header", "Value")
            .body(Body::empty())
            .unwrap();

        let transforms = vec![create_transform("X-Custom-Header", "Append", Some("Appended"))];
        Transform::apply_response_transforms(&mut resp, &transforms);

        assert_eq!(resp.headers().get("X-Custom-Header").unwrap(), "ValueAppended");
    }

    #[tokio::test]
    async fn test_apply_header_transforms_unknown_operation() {
        let mut headers = HeaderMap::new();
        headers.insert("X-Custom-Header", HeaderValue::from_static("Value"));

        let transforms = vec![create_transform("X-Custom-Header", "UnknownOp", Some("Value"))];
        Transform::apply_header_transforms(&mut headers, &transforms);

        assert_eq!(headers.get("X-Custom-Header").unwrap(), "Value");
    }
}
