//! Authentication middleware
//!
//! Axum middleware for protecting routes with OIDC authentication.

use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::Next,
    response::Response,
};
use std::sync::Arc;

use super::{AccessPolicy, AuthUser, OidcClient, PolicyResult};

/// Shared authentication state for middleware
#[derive(Clone)]
pub struct AuthMiddlewareState {
    pub client: Arc<OidcClient>,
    pub policy: AccessPolicy,
}

/// Authentication middleware function
///
/// Validates session cookie and checks access policy.
/// Sets `x-auth-user` header with user info on success.
pub async fn auth_middleware(
    state: AuthMiddlewareState,
    mut request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let cookie_name = &state.client.config().session.cookie_name;

    // Extract session cookie
    let cookies = request
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");

    let session = cookies
        .split(';')
        .filter_map(|c| {
            let mut parts = c.trim().splitn(2, '=');
            let name = parts.next()?;
            let value = parts.next()?;
            if name == cookie_name {
                Some(value)
            } else {
                None
            }
        })
        .next();

    // Decode and validate session
    let user: AuthUser = match session {
        Some(encoded) => {
            let decoded = base64_decode(encoded).map_err(|_| StatusCode::UNAUTHORIZED)?;
            serde_json::from_str(&decoded).map_err(|_| StatusCode::UNAUTHORIZED)?
        }
        None => return Err(StatusCode::UNAUTHORIZED),
    };

    // Check expiration
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    if user.exp > 0 && user.exp < now {
        return Err(StatusCode::UNAUTHORIZED);
    }

    // Check access policy
    let client_ip = request
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .or_else(|| {
            request
                .headers()
                .get("x-real-ip")
                .and_then(|v| v.to_str().ok())
        });

    match state.policy.check(&user, client_ip) {
        PolicyResult::Allowed => {
            // Add user info to request extensions for handlers
            request.extensions_mut().insert(user);
            Ok(next.run(request).await)
        }
        PolicyResult::Denied(reason) => {
            tracing::warn!(reason = %reason, "Access denied by policy");
            Err(StatusCode::FORBIDDEN)
        }
    }
}

/// Simple base64 decoding for cookies
fn base64_decode(encoded: &str) -> Result<String, &'static str> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| "Invalid base64")?;
    String::from_utf8(bytes).map_err(|_| "Invalid UTF-8")
}

/// Extract authenticated user from request extensions
pub fn get_auth_user(request: &Request) -> Option<&AuthUser> {
    request.extensions().get::<AuthUser>()
}
