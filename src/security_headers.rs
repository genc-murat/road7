use hyper::header::{HeaderMap, HeaderName, HeaderValue, STRICT_TRANSPORT_SECURITY, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS, CONTENT_SECURITY_POLICY, X_XSS_PROTECTION, REFERRER_POLICY};
use serde::{Deserialize, Serialize};
use tracing::info;

pub const PERMISSIONS_POLICY: HeaderName = HeaderName::from_static("permissions-policy");
pub const FEATURE_POLICY: HeaderName = HeaderName::from_static("feature-policy");

#[derive(Debug, Deserialize, Serialize, Clone)]
pub struct SecurityHeadersConfig {
    pub strict_transport_security: Option<String>,
    pub x_content_type_options: Option<String>,
    pub x_frame_options: Option<String>,
    pub content_security_policy: Option<String>,
    pub x_xss_protection: Option<String>,
    pub referrer_policy: Option<String>,
    pub permissions_policy: Option<String>,
    pub feature_policy: Option<String>,
}

pub async fn apply_security_headers(headers: &mut HeaderMap, security_headers_config: &Option<SecurityHeadersConfig>) {
    if let Some(config) = security_headers_config.as_ref() {
        insert_header(headers, STRICT_TRANSPORT_SECURITY, &config.strict_transport_security);
        insert_header(headers, X_CONTENT_TYPE_OPTIONS, &config.x_content_type_options);
        insert_header(headers, X_FRAME_OPTIONS, &config.x_frame_options);
        
        if let Some(csp) = &config.content_security_policy {
            headers.insert(CONTENT_SECURITY_POLICY, HeaderValue::from_str(csp).unwrap());
        } else {
            let default_csp = "default-src 'self'; script-src 'self'; object-src 'none'; style-src 'self'; img-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'";
            headers.insert(CONTENT_SECURITY_POLICY, HeaderValue::from_str(default_csp).unwrap());
        }
        
        if let Some(xss_protection) = &config.x_xss_protection {
            headers.insert(X_XSS_PROTECTION, HeaderValue::from_str(xss_protection).unwrap());
        } else {
            headers.insert(X_XSS_PROTECTION, HeaderValue::from_static("1; mode=block"));
        }
        
        if let Some(referrer_policy) = &config.referrer_policy {
            headers.insert(REFERRER_POLICY, HeaderValue::from_str(referrer_policy).unwrap());
        } else {
            headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
        }
        
        insert_header(headers, PERMISSIONS_POLICY, &config.permissions_policy);
        insert_header(headers, FEATURE_POLICY, &config.feature_policy);
        
        info!("Applied security headers: {:?}", headers);
    }
}

fn insert_header(headers: &mut HeaderMap, header_name: HeaderName, header_value: &Option<String>) {
    if let Some(value) = header_value {
        headers.insert(header_name, HeaderValue::from_str(value).unwrap());
    }
}
