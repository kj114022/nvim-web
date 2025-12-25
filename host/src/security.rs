//! Security hardening for nvim-web
//!
//! Rate limiting, origin validation, and connection throttling.

use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

/// Rate limiter for connection attempts
pub struct RateLimiter {
    /// Max attempts per window
    max_attempts: usize,
    /// Time window
    window: Duration,
    /// Attempts per IP
    attempts: HashMap<IpAddr, Vec<Instant>>,
}

impl RateLimiter {
    pub fn new(max_attempts: usize, window_secs: u64) -> Self {
        Self {
            max_attempts,
            window: Duration::from_secs(window_secs),
            attempts: HashMap::new(),
        }
    }

    /// Check if an IP is allowed to connect
    pub fn check(&mut self, ip: IpAddr) -> bool {
        let now = Instant::now();
        let attempts = self.attempts.entry(ip).or_default();

        // Prune old attempts
        attempts.retain(|t| now.duration_since(*t) < self.window);

        if attempts.len() >= self.max_attempts {
            eprintln!("SECURITY: Rate limit exceeded for {}", ip);
            return false;
        }

        attempts.push(now);
        true
    }

    /// Cleanup stale entries
    pub fn cleanup(&mut self) {
        let now = Instant::now();
        self.attempts.retain(|_, attempts| {
            attempts.retain(|t| now.duration_since(*t) < self.window);
            !attempts.is_empty()
        });
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        // 10 attempts per 60 seconds
        Self::new(10, 60)
    }
}

/// Origin validator for WebSocket connections
pub struct OriginValidator {
    allowed_origins: Vec<String>,
    allow_localhost: bool,
}

impl OriginValidator {
    pub fn new() -> Self {
        Self {
            allowed_origins: Vec::new(),
            allow_localhost: true,
        }
    }

    /// Add an allowed origin
    pub fn allow(&mut self, origin: &str) {
        self.allowed_origins.push(origin.to_string());
    }

    /// Set whether localhost is allowed
    pub fn set_allow_localhost(&mut self, allow: bool) {
        self.allow_localhost = allow;
    }

    /// Check if an origin is allowed
    pub fn check(&self, origin: &str) -> bool {
        // Always allow localhost connections for development
        if self.allow_localhost
            && (origin.contains("localhost")
                || origin.contains("127.0.0.1")
                || origin.contains("0.0.0.0"))
        {
            return true;
        }

        // Check explicit allowlist
        if self.allowed_origins.contains(&origin.to_string()) {
            return true;
        }

        // Check pattern matches (wildcard domains)
        for allowed in &self.allowed_origins {
            if let Some(domain) = allowed.strip_prefix("*.") {
                if origin.ends_with(domain) {
                    return true;
                }
            }
        }

        eprintln!("SECURITY: Origin rejected: {}", origin);
        false
    }
}

impl Default for OriginValidator {
    fn default() -> Self {
        Self::new()
    }
}

/// Security configuration
#[derive(Debug, Clone)]
pub struct SecurityConfig {
    /// Enable rate limiting
    pub rate_limit_enabled: bool,
    /// Max connections per IP per minute
    pub rate_limit_max: usize,
    /// Enable origin validation
    pub origin_check_enabled: bool,
    /// Allowed origins
    pub allowed_origins: Vec<String>,
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            rate_limit_enabled: true,
            rate_limit_max: 10,
            origin_check_enabled: true,
            allowed_origins: vec![
                "http://localhost:8080".to_string(),
                "https://localhost:8080".to_string(),
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter() {
        let mut limiter = RateLimiter::new(3, 60);
        let ip: IpAddr = "192.168.1.1".parse().unwrap();

        assert!(limiter.check(ip));
        assert!(limiter.check(ip));
        assert!(limiter.check(ip));
        assert!(!limiter.check(ip)); // 4th attempt blocked
    }

    #[test]
    fn test_origin_validator() {
        let mut validator = OriginValidator::new();
        validator.allow("https://example.com");
        validator.allow("*.trusted.com");

        assert!(validator.check("http://localhost:8080"));
        assert!(validator.check("https://example.com"));
        assert!(validator.check("https://sub.trusted.com"));
        assert!(!validator.check("https://malicious.com"));
    }
}
