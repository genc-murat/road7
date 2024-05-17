use serde::{Deserialize, Serialize};
use hyper::header::{HeaderMap, HeaderName};
use std::borrow::Cow;
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
                        let header_value = transform.value
                            .as_deref()
                            .map_or(Cow::Borrowed(""), Cow::from);
                        headers.insert(header_name, hyper::header::HeaderValue::from_str(&header_value).unwrap_or_else(|_| hyper::header::HeaderValue::from_static("")));
                    }
                }
                "Remove" => {
                    if let Ok(header_name) = transform.name.parse::<HeaderName>() {
                        headers.remove(header_name);
                    }
                }
                "Append" => {
                    if let Ok(header_name) = transform.name.parse::<HeaderName>() {
                        if let Some(current_value) = headers.get(&header_name).cloned() {
                            let new_value = format!(
                                "{}{}",
                                current_value.to_str().unwrap_or(""),
                                transform.value.as_ref().unwrap_or(&"".to_string())
                            );
                            if let Ok(new_header_value) = hyper::header::HeaderValue::from_str(&new_value) {
                                headers.insert(header_name, new_header_value);
                            }
                        } else {
                            let header_value = transform.value
                                .as_deref()
                                .map_or(hyper::header::HeaderValue::from_static(""), |v| hyper::header::HeaderValue::from_str(v).unwrap_or_else(|_| hyper::header::HeaderValue::from_static("")));
                            headers.insert(header_name, header_value);
                        }
                    }
                }
                _ => warn!("Unknown transform operation: {}", transform.operation),
            }
        }
    }
}
