use serde::{Deserialize, Serialize};
use hyper::header::{HeaderMap, HeaderName,HeaderValue};
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
            match transform.operation.as_str() {
                "Set" => {
                    if let Ok(header_name) = transform.name.parse::<HeaderName>() {
                        let header_value = transform.value.as_deref().unwrap_or("");
                        match HeaderValue::from_str(header_value) {
                            Ok(value) => { headers.insert(header_name, value); },
                            Err(_) => { headers.insert(header_name, HeaderValue::from_static("")); },
                        }
                    }
                },
                "Remove" => {
                    if let Ok(header_name) = transform.name.parse::<HeaderName>() {
                        headers.remove(header_name);
                    }
                },
                "Append" => {
                    if let Ok(header_name) = transform.name.parse::<HeaderName>() {
                        let new_value = if let Some(current_value) = headers.get(&header_name) {
                            format!("{}{}", current_value.to_str().unwrap_or(""), transform.value.as_deref().unwrap_or(""))
                        } else {
                            transform.value.as_deref().unwrap_or("").to_string()
                        };
                        
                        match HeaderValue::from_str(&new_value) {
                            Ok(value) => { headers.insert(header_name, value); },
                            Err(_) => { headers.insert(header_name, HeaderValue::from_static("")); },
                        }
                    }
                },
                _ => warn!("Unknown transform operation: {}", transform.operation),
            }
        }
    }
    
}
