//! Protocol utilities for WebSocket message parsing
//!
//! Handles URI parsing and origin validation.

use url::Url;

/// Allowed origins for WebSocket connections
/// Only localhost is allowed by default for security
pub const ALLOWED_ORIGINS: &[&str] = &[
    "http://localhost",
    "http://127.0.0.1",
    "https://localhost",
    "https://127.0.0.1",
];

/// Parse session ID from URI query string
/// Format: /?session=<id> or /?session=new
pub fn parse_session_id_from_uri(uri: &str) -> Option<String> {
    if let Some(query_start) = uri.find('?') {
        let query = &uri[query_start + 1..];
        for param in query.split('&') {
            if let Some(eq_pos) = param.find('=') {
                let key = &param[..eq_pos];
                let value = &param[eq_pos + 1..];
                if key == "session" {
                    if value == "new" {
                        return None; // Explicit request for new session
                    }
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

/// Parse view ID from URI query string (read-only viewer mode)
/// Format: /?view=<session_id>
pub fn parse_view_id_from_uri(uri: &str) -> Option<String> {
    if let Some(query_start) = uri.find('?') {
        let query = &uri[query_start + 1..];
        for param in query.split('&') {
            if let Some(eq_pos) = param.find('=') {
                let key = &param[..eq_pos];
                let value = &param[eq_pos + 1..];
                if key == "view" && !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }
    None
}

/// Parse context URL from URI query string
/// Format: /?context=<url_encoded_url>
pub fn parse_context_from_uri(uri: &str) -> Option<String> {
    if let Some(query_start) = uri.find('?') {
        let query = &uri[query_start + 1..];
        for param in query.split('&') {
            if let Some(eq_pos) = param.find('=') {
                let key = &param[..eq_pos];
                let value = &param[eq_pos + 1..];
                if key == "context" && !value.is_empty() {
                    // Percent decode the value
                    if let Some(_decoded) = url::form_urlencoded::parse(value.as_bytes())
                        .map(|(k, _)| k.to_string())
                        .next() 
                    {
                        // Note: form_urlencoded::parse returns key-value pairs
                        // If we just have a value, we need to treat it carefully.
                        // Actually, percent_encoding crate is better but url::form_urlencoded handles + vs %20 correctly.
                        // Let's use percent_encoding directly if we want raw decoding, 
                        // but since query params are form-urlencoded, let's use that.
                        // Wait, url::form_urlencoded::parse is for key=value pairs.
                        // We just want to decode a single string.
                        // Use percent_encoding::percent_decode_str
                        return percent_encoding::percent_decode_str(value)
                            .decode_utf8()
                            .ok()
                            .map(|s| s.into_owned());
                    }
                }
            }
        }
    }
    None
}

/// Validate origin header against whitelist using strict URL parsing
///
/// This function parses both the origin and allowed origins as URLs,
/// then compares scheme and host exactly. This prevents bypass attacks
/// like `http://localhost.evil.com` which would pass a naive `starts_with` check.
pub fn validate_origin(origin: &str) -> bool {
    let Ok(origin_url) = Url::parse(origin) else {
        return false;
    };

    let origin_host = origin_url.host_str().unwrap_or("");
    let origin_scheme = origin_url.scheme();

    ALLOWED_ORIGINS.iter().any(|allowed| {
        let Ok(allowed_url) = Url::parse(allowed) else {
            return false;
        };

        let allowed_host = allowed_url.host_str().unwrap_or("");
        let allowed_scheme = allowed_url.scheme();

        // Strict comparison: scheme and host must match exactly
        origin_scheme == allowed_scheme && origin_host == allowed_host
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_session_id() {
        assert_eq!(parse_session_id_from_uri("/?session=abc123"), Some("abc123".to_string()));
        assert_eq!(parse_session_id_from_uri("/?session=new"), None);
        assert_eq!(parse_session_id_from_uri("/"), None);
        assert_eq!(parse_session_id_from_uri("/?foo=bar&session=xyz"), Some("xyz".to_string()));
    }

    #[test]
    fn test_validate_origin_valid() {
        // Valid localhost origins
        assert!(validate_origin("http://localhost"));
        assert!(validate_origin("http://localhost:8080"));
        assert!(validate_origin("http://127.0.0.1"));
        assert!(validate_origin("http://127.0.0.1:3000"));
        assert!(validate_origin("https://localhost"));
        assert!(validate_origin("https://localhost:443"));
    }

    #[test]
    fn test_validate_origin_bypass_attempts() {
        // Bypass attempts that MUST be rejected
        assert!(!validate_origin("http://localhost.evil.com"));
        assert!(!validate_origin("http://localhost.evil.com:8080"));
        assert!(!validate_origin("http://evil.localhost.com"));
        assert!(!validate_origin("http://localhostevil.com"));
        assert!(!validate_origin("http://127.0.0.1.evil.com"));
    }

    #[test]
    fn test_validate_origin_invalid() {
        // Other invalid origins
        assert!(!validate_origin("http://evil.com"));
        assert!(!validate_origin("http://192.168.1.1"));
        assert!(!validate_origin("http://example.com"));
        assert!(!validate_origin("not-a-url"));
        assert!(!validate_origin(""));
    }

    #[test]
    fn test_parse_context() {
        assert_eq!(
            parse_context_from_uri("/?context=https%3A%2F%2Fgithub.com"), 
            Some("https://github.com".to_string())
        );
        assert_eq!(parse_context_from_uri("/?foo=bar"), None);
    }
}

