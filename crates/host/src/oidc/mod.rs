//! OIDC (OpenID Connect) Authentication
//!
//! Provides enterprise SSO authentication with support for:
//! - Authorization code flow with PKCE
//! - Token validation and refresh
//! - BeyondCorp-style access policies

mod client;
mod config;
mod middleware;
mod routes;

pub use client::OidcClient;
pub use config::AuthConfig;
pub use middleware::auth_middleware;
pub use routes::auth_routes;

use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Authenticated user information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    /// Unique user identifier (sub claim)
    pub sub: String,
    /// User's email address
    pub email: Option<String>,
    /// User's display name
    pub name: Option<String>,
    /// Groups the user belongs to
    pub groups: Vec<String>,
    /// Token expiration timestamp
    pub exp: u64,
}

impl AuthUser {
    /// Check if user belongs to a specific group
    pub fn has_group(&self, group: &str) -> bool {
        self.groups.iter().any(|g| g == group)
    }

    /// Check if user's email matches a domain
    pub fn has_domain(&self, domain: &str) -> bool {
        self.email
            .as_ref()
            .map(|e| e.ends_with(&format!("@{domain}")))
            .unwrap_or(false)
    }
}

/// BeyondCorp access policy
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccessPolicy {
    /// Allowed email domains
    #[serde(default)]
    pub allowed_domains: HashSet<String>,
    /// Allowed groups
    #[serde(default)]
    pub allowed_groups: HashSet<String>,
    /// Allowed IP ranges (CIDR notation)
    #[serde(default)]
    pub allowed_ips: HashSet<String>,
    /// Require specific claims
    #[serde(default)]
    pub required_claims: HashSet<String>,
}

impl AccessPolicy {
    /// Check if user passes policy
    pub fn check(&self, user: &AuthUser, client_ip: Option<&str>) -> PolicyResult {
        // Check domain
        if !self.allowed_domains.is_empty() {
            let domain_ok = user
                .email
                .as_ref()
                .and_then(|e| e.split('@').last())
                .map(|d| self.allowed_domains.contains(d))
                .unwrap_or(false);

            if !domain_ok {
                return PolicyResult::Denied("Email domain not allowed".to_string());
            }
        }

        // Check groups
        if !self.allowed_groups.is_empty() {
            let group_ok = user.groups.iter().any(|g| self.allowed_groups.contains(g));
            if !group_ok {
                return PolicyResult::Denied("User not in allowed group".to_string());
            }
        }

        // Check IP (simple prefix matching for now)
        if !self.allowed_ips.is_empty() {
            if let Some(ip) = client_ip {
                let ip_ok = self.allowed_ips.iter().any(|cidr| ip.starts_with(cidr));
                if !ip_ok {
                    return PolicyResult::Denied("IP address not allowed".to_string());
                }
            }
        }

        PolicyResult::Allowed
    }
}

/// Result of policy check
#[derive(Debug, Clone)]
pub enum PolicyResult {
    Allowed,
    Denied(String),
}

impl PolicyResult {
    pub fn is_allowed(&self) -> bool {
        matches!(self, PolicyResult::Allowed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_user_has_domain() {
        let user = AuthUser {
            sub: "user123".to_string(),
            email: Some("alice@example.com".to_string()),
            name: Some("Alice".to_string()),
            groups: vec!["engineering".to_string()],
            exp: 9999999999,
        };

        assert!(user.has_domain("example.com"));
        assert!(!user.has_domain("other.com"));
    }

    #[test]
    fn test_policy_check_domain() {
        let policy = AccessPolicy {
            allowed_domains: ["example.com".to_string()].into_iter().collect(),
            ..Default::default()
        };

        let user_allowed = AuthUser {
            sub: "u1".to_string(),
            email: Some("alice@example.com".to_string()),
            name: None,
            groups: vec![],
            exp: 0,
        };

        let user_denied = AuthUser {
            sub: "u2".to_string(),
            email: Some("bob@other.com".to_string()),
            name: None,
            groups: vec![],
            exp: 0,
        };

        assert!(policy.check(&user_allowed, None).is_allowed());
        assert!(!policy.check(&user_denied, None).is_allowed());
    }

    #[test]
    fn test_policy_check_groups() {
        let policy = AccessPolicy {
            allowed_groups: ["admin".to_string()].into_iter().collect(),
            ..Default::default()
        };

        let admin = AuthUser {
            sub: "u1".to_string(),
            email: None,
            name: None,
            groups: vec!["admin".to_string()],
            exp: 0,
        };

        let regular = AuthUser {
            sub: "u2".to_string(),
            email: None,
            name: None,
            groups: vec!["users".to_string()],
            exp: 0,
        };

        assert!(policy.check(&admin, None).is_allowed());
        assert!(!policy.check(&regular, None).is_allowed());
    }
}
