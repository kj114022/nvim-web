//! OIDC client configuration
//!
//! Supports multiple identity providers (Google, Okta, Azure AD, etc.)

use serde::{Deserialize, Serialize};

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Enable authentication
    #[serde(default)]
    pub enabled: bool,

    /// OIDC issuer URL (e.g., https://accounts.google.com)
    pub issuer: String,

    /// OAuth2 client ID
    pub client_id: String,

    /// OAuth2 client secret (None for PKCE-only flows)
    #[serde(default)]
    pub client_secret: Option<String>,

    /// Redirect URI for OAuth callback
    pub redirect_uri: String,

    /// Scopes to request (default: openid email profile)
    #[serde(default = "default_scopes")]
    pub scopes: Vec<String>,

    /// Session cookie settings
    #[serde(default)]
    pub session: SessionConfig,

    /// BeyondCorp access policy
    #[serde(default)]
    pub policy: super::AccessPolicy,
}

fn default_scopes() -> Vec<String> {
    vec![
        "openid".to_string(),
        "email".to_string(),
        "profile".to_string(),
    ]
}

/// Session cookie configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionConfig {
    /// Cookie name
    #[serde(default = "default_cookie_name")]
    pub cookie_name: String,

    /// Cookie max age in seconds (default: 24 hours)
    #[serde(default = "default_max_age")]
    pub max_age_secs: u64,

    /// Secure cookie (HTTPS only)
    #[serde(default = "default_true")]
    pub secure: bool,

    /// HTTP-only cookie
    #[serde(default = "default_true")]
    pub http_only: bool,

    /// SameSite policy
    #[serde(default = "default_same_site")]
    pub same_site: String,
}

fn default_cookie_name() -> String {
    "nvim_web_session".to_string()
}

fn default_max_age() -> u64 {
    86400 // 24 hours
}

fn default_true() -> bool {
    true
}

fn default_same_site() -> String {
    "Lax".to_string()
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            cookie_name: default_cookie_name(),
            max_age_secs: default_max_age(),
            secure: true,
            http_only: true,
            same_site: default_same_site(),
        }
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            issuer: String::new(),
            client_id: String::new(),
            client_secret: None,
            redirect_uri: String::new(),
            scopes: default_scopes(),
            session: SessionConfig::default(),
            policy: super::AccessPolicy::default(),
        }
    }
}

/// Preset configurations for common providers
impl AuthConfig {
    /// Google OIDC configuration
    pub fn google(client_id: &str, client_secret: &str, redirect_uri: &str) -> Self {
        Self {
            enabled: true,
            issuer: "https://accounts.google.com".to_string(),
            client_id: client_id.to_string(),
            client_secret: Some(client_secret.to_string()),
            redirect_uri: redirect_uri.to_string(),
            scopes: vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
            ],
            session: SessionConfig::default(),
            policy: super::AccessPolicy::default(),
        }
    }

    /// Okta configuration
    pub fn okta(domain: &str, client_id: &str, client_secret: &str, redirect_uri: &str) -> Self {
        Self {
            enabled: true,
            issuer: format!("https://{domain}"),
            client_id: client_id.to_string(),
            client_secret: Some(client_secret.to_string()),
            redirect_uri: redirect_uri.to_string(),
            scopes: vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
                "groups".to_string(),
            ],
            session: SessionConfig::default(),
            policy: super::AccessPolicy::default(),
        }
    }

    /// Azure AD configuration
    pub fn azure_ad(
        tenant_id: &str,
        client_id: &str,
        client_secret: &str,
        redirect_uri: &str,
    ) -> Self {
        Self {
            enabled: true,
            issuer: format!("https://login.microsoftonline.com/{tenant_id}/v2.0"),
            client_id: client_id.to_string(),
            client_secret: Some(client_secret.to_string()),
            redirect_uri: redirect_uri.to_string(),
            scopes: vec![
                "openid".to_string(),
                "email".to_string(),
                "profile".to_string(),
            ],
            session: SessionConfig::default(),
            policy: super::AccessPolicy::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_google_preset() {
        let config = AuthConfig::google("client123", "secret456", "http://localhost:8080/callback");
        assert_eq!(config.issuer, "https://accounts.google.com");
        assert!(config.enabled);
    }

    #[test]
    fn test_session_defaults() {
        let session = SessionConfig::default();
        assert_eq!(session.cookie_name, "nvim_web_session");
        assert!(session.secure);
        assert!(session.http_only);
    }
}
