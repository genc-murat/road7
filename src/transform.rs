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
