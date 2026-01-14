//! Authentication routes for OIDC flow
//!
//! Provides HTTP endpoints for:
//! - /auth/login - Initiate login flow
//! - /auth/callback - OAuth2 callback
//! - /auth/logout - End session
//! - /auth/me - Get current user info

use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Json, Router,
};
use serde::Deserialize;
use std::sync::Arc;

use super::{AuthUser, OidcClient};

/// Shared OIDC client state
pub type SharedOidcClient = Arc<OidcClient>;

/// Auth routes state
#[derive(Clone)]
pub struct AuthState {
    pub client: SharedOidcClient,
}

/// Create auth routes
pub fn auth_routes(client: SharedOidcClient) -> Router {
    let state = AuthState { client };

    Router::new()
        .route("/login", get(login))
        .route("/callback", get(callback))
        .route("/logout", get(logout))
        .route("/me", get(me))
        .with_state(state)
}

/// Login endpoint - redirects to OIDC provider
async fn login(State(state): State<AuthState>) -> impl IntoResponse {
    let (url, _state) = state.client.authorize_url().await;
    Redirect::temporary(&url)
}

/// OAuth2 callback query parameters
#[derive(Debug, Deserialize)]
struct CallbackParams {
    code: String,
    state: String,
}

/// Callback endpoint - exchanges code for tokens
async fn callback(
    State(state): State<AuthState>,
    Query(params): Query<CallbackParams>,
) -> Response {
    match state
        .client
        .exchange_code(&params.code, &params.state)
        .await
    {
        Ok(user) => {
            // Create session cookie with user info
            let session_data = serde_json::to_string(&user).unwrap_or_default();
            let cookie_config = &state.client.config().session;

            let cookie_value = format!(
                "{}={}; Max-Age={}; Path=/; {}{}SameSite={}",
                cookie_config.cookie_name,
                base64_encode(&session_data),
                cookie_config.max_age_secs,
                if cookie_config.secure { "Secure; " } else { "" },
                if cookie_config.http_only {
                    "HttpOnly; "
                } else {
                    ""
                },
                cookie_config.same_site
            );

            // Redirect to home with session cookie
            Response::builder()
                .status(StatusCode::FOUND)
                .header(header::LOCATION, "/")
                .header(header::SET_COOKIE, cookie_value)
                .body(axum::body::Body::empty())
                .unwrap()
        }
        Err(e) => {
            tracing::error!(error = %e, "OAuth callback failed");
            Response::builder()
                .status(StatusCode::UNAUTHORIZED)
                .body(axum::body::Body::from(format!(
                    "Authentication failed: {e}"
                )))
                .unwrap()
        }
    }
}

/// Logout endpoint - clears session
async fn logout(State(state): State<AuthState>) -> Response {
    let cookie_name = &state.client.config().session.cookie_name;

    // Clear cookie by setting it to empty with immediate expiration
    let cookie_value = format!(
        "{}=; Max-Age=0; Path=/; Secure; HttpOnly; SameSite=Lax",
        cookie_name
    );

    Response::builder()
        .status(StatusCode::FOUND)
        .header(header::LOCATION, "/")
        .header(header::SET_COOKIE, cookie_value)
        .body(axum::body::Body::empty())
        .unwrap()
}

/// Get current user info
async fn me(
    State(state): State<AuthState>,
    headers: axum::http::HeaderMap,
) -> Result<Json<AuthUser>, StatusCode> {
    let cookie_name = &state.client.config().session.cookie_name;

    // Extract session cookie
    let cookies = headers
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

    match session {
        Some(encoded) => {
            let decoded = base64_decode(encoded).map_err(|_| StatusCode::UNAUTHORIZED)?;
            let user: AuthUser =
                serde_json::from_str(&decoded).map_err(|_| StatusCode::UNAUTHORIZED)?;

            // Check if session is expired
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(0);

            if user.exp > 0 && user.exp < now {
                return Err(StatusCode::UNAUTHORIZED);
            }

            Ok(Json(user))
        }
        None => Err(StatusCode::UNAUTHORIZED),
    }
}

/// Simple base64 encoding for cookies
fn base64_encode(data: &str) -> String {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    URL_SAFE_NO_PAD.encode(data.as_bytes())
}

/// Simple base64 decoding for cookies
fn base64_decode(encoded: &str) -> Result<String, &'static str> {
    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
    let bytes = URL_SAFE_NO_PAD
        .decode(encoded)
        .map_err(|_| "Invalid base64")?;
    String::from_utf8(bytes).map_err(|_| "Invalid UTF-8")
}
