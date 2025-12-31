//! Rate limiting for WebSocket connections
//!
//! Implements a token bucket algorithm to prevent DoS attacks.
//! Default: 1000 token burst, 100 tokens/second refill.

use std::time::Instant;

/// Token bucket rate limiter
///
/// Allows burst traffic up to `max_tokens`, then limits to `refill_rate` per second.
pub struct RateLimiter {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
    last_refill: Instant,
}

impl RateLimiter {
    /// Create a new rate limiter
    ///
    /// # Arguments
    /// * `max_tokens` - Maximum burst size
    /// * `refill_rate` - Tokens added per second
    pub fn new(max_tokens: f64, refill_rate: f64) -> Self {
        Self {
            tokens: max_tokens,
            max_tokens,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    /// Create with default settings (1000 burst, 100/sec)
    pub fn default_ws() -> Self {
        Self::new(1000.0, 100.0)
    }

    /// Try to consume one token
    ///
    /// Returns `true` if allowed, `false` if rate limited.
    pub fn try_consume(&mut self) -> bool {
        self.refill();
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Refill tokens based on elapsed time
    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_refill = now;
    }

    /// Check if currently rate limited (without consuming)
    #[allow(dead_code)]
    pub fn is_limited(&mut self) -> bool {
        self.refill();
        self.tokens < 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn test_burst_allowed() {
        let mut limiter = RateLimiter::new(10.0, 1.0);
        // Should allow 10 requests
        for _ in 0..10 {
            assert!(limiter.try_consume());
        }
        // 11th should fail
        assert!(!limiter.try_consume());
    }

    #[test]
    fn test_refill() {
        let mut limiter = RateLimiter::new(10.0, 10.0); // 10/sec refill
        // Consume all
        for _ in 0..10 {
            limiter.try_consume();
        }
        assert!(!limiter.try_consume());
        
        // Wait 200ms = 2 tokens refilled
        sleep(Duration::from_millis(200));
        assert!(limiter.try_consume());
        assert!(limiter.try_consume());
        assert!(!limiter.try_consume());
    }
}
