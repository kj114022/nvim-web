//! OIDC client implementation
//!
//! Handles OAuth2/OIDC flows with a simple reqwest-based implementation.
//! For production use, consider using the openidconnect crate with full
//! OIDC discovery and JWT validation.

use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::config::AuthConfig;
use super::AuthUser;

/// OIDC client for authentication flows
pub struct OidcClient {
    /// HTTP client
    http_client: reqwest::Client,
    /// Configuration
    config: AuthConfig,
    /// Pending authentication states (state -> verifier)
    pending: Arc<RwLock<HashMap<String, PendingAuth>>>,
}

/// Pending authentication state
struct PendingAuth {
    code_verifier: String,
    created_at: std::time::Instant,
}

/// Token response from OAuth2 provider
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct TokenResponse {
    access_token: String,
    token_type: String,
    #[serde(default)]
    expires_in: Option<u64>,
    #[serde(default)]
    id_token: Option<String>,
}

/// UserInfo response
#[derive(Debug, Deserialize)]
struct UserInfoResponse {
    sub: String,
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    name: Option<String>,
}

impl OidcClient {
    /// Create a new OIDC client from configuration
    pub async fn new(config: AuthConfig) -> Result<Self> {
        let http_client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?;

        Ok(Self {
            http_client,
            config,
            pending: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Generate authorization URL for login
    pub async fn authorize_url(&self) -> (String, String) {
        // Generate PKCE code verifier and challenge
        let code_verifier = generate_code_verifier();
        let code_challenge = generate_code_challenge(&code_verifier);
        let state = generate_random_string(32);

        // Build authorization URL
        let auth_url = format!(
            "{}/authorize?response_type=code&client_id={}&redirect_uri={}&scope={}&state={}&code_challenge={}&code_challenge_method=S256",
            self.config.issuer,
            urlencoding::encode(&self.config.client_id),
            urlencoding::encode(&self.config.redirect_uri),
            urlencoding::encode(&self.config.scopes.join(" ")),
            urlencoding::encode(&state),
            urlencoding::encode(&code_challenge),
        );

        // Store pending auth
        {
            let mut pending = self.pending.write().await;
            pending.insert(
                state.clone(),
                PendingAuth {
                    code_verifier,
                    created_at: std::time::Instant::now(),
                },
            );
            // Clean up old states
            pending.retain(|_, v| v.created_at.elapsed().as_secs() < 600);
        }

        (auth_url, state)
    }

    /// Exchange authorization code for tokens
    pub async fn exchange_code(&self, code: &str, state: &str) -> Result<AuthUser> {
        // Get pending auth state
        let pending_auth = {
            let mut pending = self.pending.write().await;
            pending
                .remove(state)
                .ok_or_else(|| anyhow::anyhow!("Invalid or expired state"))?
        };

        // Build token request
        let token_url = format!("{}/oauth/token", self.config.issuer);
        let mut params = HashMap::new();
        params.insert("grant_type", "authorization_code");
        params.insert("code", code);
        params.insert("redirect_uri", &self.config.redirect_uri);
        params.insert("client_id", &self.config.client_id);
        params.insert("code_verifier", &pending_auth.code_verifier);

        let mut request = self.http_client.post(&token_url).form(&params);

        // Add client secret if configured
        if let Some(ref secret) = self.config.client_secret {
            request = request.basic_auth(&self.config.client_id, Some(secret));
        }

        // Exchange code for token
        let response = request.send().await?;
        if !response.status().is_success() {
            let error = response.text().await.unwrap_or_default();
            anyhow::bail!("Token exchange failed: {}", error);
        }

        let token: TokenResponse = response.json().await?;

        // Calculate expiration
        let exp = token
            .expires_in
            .map(|d| {
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|t| t.as_secs() + d)
                    .unwrap_or(0)
            })
            .unwrap_or(0);

        // Try to fetch user info
        let user = self
            .fetch_userinfo(&token.access_token)
            .await
            .unwrap_or_else(|_| AuthUser {
                sub: format!(
                    "user_{}",
                    &token.access_token[..8.min(token.access_token.len())]
                ),
                email: None,
                name: None,
                groups: vec![],
                exp,
            });

        Ok(AuthUser { exp, ..user })
    }

    /// Fetch user info from userinfo endpoint
    async fn fetch_userinfo(&self, access_token: &str) -> Result<AuthUser> {
        let userinfo_url = format!("{}/userinfo", self.config.issuer);

        let response = self
            .http_client
            .get(&userinfo_url)
            .bearer_auth(access_token)
            .send()
            .await?;

        if !response.status().is_success() {
            anyhow::bail!("UserInfo request failed");
        }

        let info: UserInfoResponse = response.json().await?;

        Ok(AuthUser {
            sub: info.sub,
            email: info.email,
            name: info.name,
            groups: vec![],
            exp: 0,
        })
    }

    /// Get configuration
    pub fn config(&self) -> &AuthConfig {
        &self.config
    }
}

/// Generate random code verifier for PKCE
fn generate_code_verifier() -> String {
    generate_random_string(64)
}

/// Generate code challenge from verifier (S256)
fn generate_code_challenge(verifier: &str) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(verifier.as_bytes());
    let hash = hasher.finalize();
    URL_SAFE_NO_PAD.encode(hash)
}

/// Generate random string for state/verifier
fn generate_random_string(len: usize) -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..len)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_code_challenge() {
        let verifier = "test_verifier_12345678901234567890123456789012345678901234";
        let challenge = generate_code_challenge(verifier);
        assert!(!challenge.is_empty());
        assert!(challenge.len() > 20);
    }

    #[test]
    fn test_random_string() {
        let s1 = generate_random_string(32);
        let s2 = generate_random_string(32);
        assert_eq!(s1.len(), 32);
        assert_ne!(s1, s2);
    }
}
