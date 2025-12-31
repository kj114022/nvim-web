//! Protocol utilities for WebSocket message parsing
//!
//! Handles URI parsing and origin validation.

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

/// Validate origin header against whitelist
pub fn validate_origin(origin: &str) -> bool {
    ALLOWED_ORIGINS
        .iter()
        .any(|allowed| origin.starts_with(allowed))
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
    fn test_validate_origin() {
        assert!(validate_origin("http://localhost:8080"));
        assert!(validate_origin("http://127.0.0.1:8080"));
        assert!(!validate_origin("http://evil.com"));
    }
}
