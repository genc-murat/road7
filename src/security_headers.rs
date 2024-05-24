use hyper::header::{HeaderMap, HeaderName, HeaderValue, STRICT_TRANSPORT_SECURITY, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS, CONTENT_SECURITY_POLICY, X_XSS_PROTECTION, REFERRER_POLICY};
use serde::{Deserialize, Serialize};
use tracing::info;

pub const PERMISSIONS_POLICY: HeaderName = HeaderName::from_static("permissions-policy");
pub const FEATURE_POLICY: HeaderName = HeaderName::from_static("feature-policy");

pub const DEFAULT_CSP: &str = "default-src 'self'; script-src 'self'; object-src 'none'; style-src 'self'; img-src 'self'; frame-ancestors 'none'; base-uri 'self'; form-action 'self'";
pub const DEFAULT_X_XSS_PROTECTION: &str = "1; mode=block";
pub const DEFAULT_REFERRER_POLICY: &str = "no-referrer";

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
            headers.insert(CONTENT_SECURITY_POLICY, HeaderValue::from_str(DEFAULT_CSP).unwrap());
        }
        
        if let Some(xss_protection) = &config.x_xss_protection {
            headers.insert(X_XSS_PROTECTION, HeaderValue::from_str(xss_protection).unwrap());
        } else {
            headers.insert(X_XSS_PROTECTION, HeaderValue::from_static(DEFAULT_X_XSS_PROTECTION));
        }
        
        if let Some(referrer_policy) = &config.referrer_policy {
            headers.insert(REFERRER_POLICY, HeaderValue::from_str(referrer_policy).unwrap());
        } else {
            headers.insert(REFERRER_POLICY, HeaderValue::from_static(DEFAULT_REFERRER_POLICY));
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


#[cfg(test)]
mod tests {
    use super::*;
    use hyper::header::HeaderMap;

    #[tokio::test]
    async fn test_apply_security_headers_with_full_config() {
        let config = SecurityHeadersConfig {
            strict_transport_security: Some("max-age=31536000; includeSubDomains".to_string()),
            x_content_type_options: Some("nosniff".to_string()),
            x_frame_options: Some("DENY".to_string()),
            content_security_policy: Some("default-src 'self'".to_string()),
            x_xss_protection: Some("1; mode=block".to_string()),
            referrer_policy: Some("no-referrer".to_string()),
            permissions_policy: Some("geolocation=()".to_string()),
            feature_policy: Some("vibrate 'none'".to_string()),
        };

        let mut headers = HeaderMap::new();
        apply_security_headers(&mut headers, &Some(config)).await;

        assert_eq!(headers.get(STRICT_TRANSPORT_SECURITY).unwrap(), "max-age=31536000; includeSubDomains");
        assert_eq!(headers.get(X_CONTENT_TYPE_OPTIONS).unwrap(), "nosniff");
        assert_eq!(headers.get(X_FRAME_OPTIONS).unwrap(), "DENY");
        assert_eq!(headers.get(CONTENT_SECURITY_POLICY).unwrap(), "default-src 'self'");
        assert_eq!(headers.get(X_XSS_PROTECTION).unwrap(), "1; mode=block");
        assert_eq!(headers.get(REFERRER_POLICY).unwrap(), "no-referrer");
        assert_eq!(headers.get(PERMISSIONS_POLICY).unwrap(), "geolocation=()");
        assert_eq!(headers.get(FEATURE_POLICY).unwrap(), "vibrate 'none'");
    }

    #[tokio::test]
    async fn test_apply_security_headers_with_partial_config() {
        let config = SecurityHeadersConfig {
            strict_transport_security: Some("max-age=31536000".to_string()),
            x_content_type_options: None,
            x_frame_options: None,
            content_security_policy: None,
            x_xss_protection: None,
            referrer_policy: None,
            permissions_policy: None,
            feature_policy: None,
        };

        let mut headers = HeaderMap::new();
        apply_security_headers(&mut headers, &Some(config)).await;

        assert_eq!(headers.get(STRICT_TRANSPORT_SECURITY).unwrap(), "max-age=31536000");
        assert!(headers.get(X_CONTENT_TYPE_OPTIONS).is_none());
        assert!(headers.get(X_FRAME_OPTIONS).is_none());
        assert_eq!(headers.get(CONTENT_SECURITY_POLICY).unwrap(), DEFAULT_CSP);
        assert_eq!(headers.get(X_XSS_PROTECTION).unwrap(), DEFAULT_X_XSS_PROTECTION);
        assert_eq!(headers.get(REFERRER_POLICY).unwrap(), DEFAULT_REFERRER_POLICY);
        assert!(headers.get(PERMISSIONS_POLICY).is_none());
        assert!(headers.get(FEATURE_POLICY).is_none());
    }

    #[tokio::test]
    async fn test_apply_security_headers_with_empty_config() {
        let config = SecurityHeadersConfig {
            strict_transport_security: None,
            x_content_type_options: None,
            x_frame_options: None,
            content_security_policy: None,
            x_xss_protection: None,
            referrer_policy: None,
            permissions_policy: None,
            feature_policy: None,
        };

        let mut headers = HeaderMap::new();
        apply_security_headers(&mut headers, &Some(config)).await;

        assert!(headers.get(STRICT_TRANSPORT_SECURITY).is_none());
        assert!(headers.get(X_CONTENT_TYPE_OPTIONS).is_none());
        assert!(headers.get(X_FRAME_OPTIONS).is_none());
        assert_eq!(headers.get(CONTENT_SECURITY_POLICY).unwrap(), DEFAULT_CSP);
        assert_eq!(headers.get(X_XSS_PROTECTION).unwrap(), DEFAULT_X_XSS_PROTECTION);
        assert_eq!(headers.get(REFERRER_POLICY).unwrap(), DEFAULT_REFERRER_POLICY);
        assert!(headers.get(PERMISSIONS_POLICY).is_none());
        assert!(headers.get(FEATURE_POLICY).is_none());
    }

    #[tokio::test]
    async fn test_apply_security_headers_with_none_config() {
        let mut headers = HeaderMap::new();
        apply_security_headers(&mut headers, &None).await;

        assert!(headers.get(STRICT_TRANSPORT_SECURITY).is_none());
        assert!(headers.get(X_CONTENT_TYPE_OPTIONS).is_none());
        assert!(headers.get(X_FRAME_OPTIONS).is_none());
        assert!(headers.get(CONTENT_SECURITY_POLICY).is_none());
        assert!(headers.get(X_XSS_PROTECTION).is_none());
        assert!(headers.get(REFERRER_POLICY).is_none());
        assert!(headers.get(PERMISSIONS_POLICY).is_none());
        assert!(headers.get(FEATURE_POLICY).is_none());
    }
}
