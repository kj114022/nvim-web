//! REST API server for nvim-web
//!
//! Provides HTTP endpoints for session management and automation.

use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    routing::{delete, get, post},
    Json, Router,
};
use serde::Deserialize;
use tokio::sync::RwLock;

use crate::session::{AsyncSessionManager, SessionInfo};
use crate::vfs::SshFsBackend;

// Shared state
#[derive(Clone)]
pub struct AppState {
    pub session_manager: Arc<RwLock<AsyncSessionManager>>,
    pub ws_port: u16,
}

// SSH connection request
#[derive(Deserialize)]
pub struct SshConnectRequest {
    pub host: String,
    pub port: Option<u16>,
    pub user: String,
    pub password: Option<String>,
}

// Routes
pub fn api_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health_check))
        .route("/sessions", get(list_sessions).post(create_session))
        .route("/sessions/count", get(session_count))
        .route("/sessions/:id", delete(delete_session))
        .route("/sessions/:id/share", post(create_share_link))
        .route("/sessions/:id/shares", get(list_share_links))
        .route("/sessions/:id/snapshot", post(create_snapshot))
        .route("/sessions/:id/snapshots", get(list_snapshots))
        .route("/open", post(open_project))
        .route("/claim/:token", get(claim_token))
        .route("/token/:token", get(get_token_info))
        .route("/share/:token", get(use_share_link).delete(revoke_share_link))
        .route("/snapshot/:id", get(get_snapshot).delete(delete_snapshot_handler))
        .route("/ssh/test", post(test_ssh_connection))
        .route("/ssh/connect", post(connect_ssh))
        .route("/ssh/disconnect", post(disconnect_ssh))
}

// Handlers

async fn health_check() -> Json<serde_json::Value> {
    Json(serde_json::json!({ "status": "ok", "version": "0.1.0" }))
}

async fn list_sessions(State(state): State<AppState>) -> Json<serde_json::Value> {
    let sessions: Vec<SessionInfo> = state.session_manager.read().await.list_sessions();
    // Use serde_json::to_value to serialize the list
    Json(serde_json::json!({ "sessions": sessions }))
}

async fn session_count(State(state): State<AppState>) -> Json<serde_json::Value> {
    let mgr = state.session_manager.read().await;
    Json(serde_json::json!({ "count": mgr.session_count() }))
}

async fn create_session(State(state): State<AppState>) -> impl IntoResponse {
    let mut mgr = state.session_manager.write().await;
    match mgr.create_session(None).await {
        Ok(id) => (
            StatusCode::OK,
            Json(serde_json::json!({ "id": id, "created": true })),
        ),
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

async fn delete_session(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> impl IntoResponse {
    let mut mgr = state.session_manager.write().await;
    if mgr.remove_session(&id).is_some() {
        (
            StatusCode::OK,
            Json(serde_json::json!({ "id": id, "deleted": true })),
        )
    } else {
        (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "session not found" })),
        )
    }
}

#[derive(Deserialize)]
struct OpenRequest {
    path: String,
}

async fn open_project(
    State(_state): State<AppState>,
    Json(payload): Json<OpenRequest>,
) -> impl IntoResponse {
    let path_str = payload.path;
    if path_str.is_empty() {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "path is required" })),
        );
    }

    let abs_path = std::path::PathBuf::from(path_str);
    // Don't enforce existence strictly if we want to allow creating new projects,
    // but for now let's keep the check for safety or auto-create?
    // Existing logic checked existence.
    if !abs_path.exists() {
        // We could create it?
        // For verify step: mkdir was done.
        return (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "path does not exist" })),
        );
    }

    let abs_path = abs_path.canonicalize().unwrap_or(abs_path);
    let config = crate::project::ProjectConfig::load(&abs_path);
    let name = config.display_name(&abs_path);
    let token = crate::project::store_token(abs_path.clone(), config);

    (
        StatusCode::OK,
        Json(serde_json::json!({
            "token": token,
            "name": name,
            "path": abs_path.display().to_string(),
            "url": format!("http://localhost:8080/?open={}", token)
        })),
    )
}

async fn claim_token(Path(token): Path<String>) -> impl IntoResponse {
    match crate::project::claim_token(&token) {
        Some((path, config)) => {
            let name = config.display_name(&path);
            let cwd = config.resolved_cwd(&path);
            let init_file = config.editor.init_file.unwrap_or_default();

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "path": path.display().to_string(),
                    "name": name,
                    "cwd": cwd.display().to_string(),
                    "init_file": init_file
                })),
            )
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "token invalid or expired" })),
        ),
    }
}

async fn get_token_info(Path(token): Path<String>) -> impl IntoResponse {
    match crate::project::get_token_info(&token) {
        Some((path, config, claimed)) => {
            let name = config.display_name(&path);
            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "path": path.display().to_string(),
                    "name": name,
                    "claimed": claimed
                })),
            )
        }
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "token not found" })),
        ),
    }
}

async fn test_ssh_connection(Json(payload): Json<SshConnectRequest>) -> impl IntoResponse {
    let uri = format!(
        "vfs://ssh/{}@{}:{}/",
        payload.user,
        payload.host,
        payload.port.unwrap_or(22)
    );

    match SshFsBackend::test_connection(&uri, payload.password.as_deref()) {
        Ok(()) => (
            StatusCode::OK,
            Json(serde_json::json!({ "success": true })),
        ),
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

async fn connect_ssh(
    State(state): State<AppState>,
    Json(payload): Json<SshConnectRequest>,
) -> impl IntoResponse {
    let uri = format!(
        "vfs://ssh/{}@{}:{}/",
        payload.user,
        payload.host,
        payload.port.unwrap_or(22)
    );

    match SshFsBackend::connect_with_password(&uri, payload.password.as_deref()) {
        Ok(_backend) => {
            // Store the active SSH connection in session manager
            state.session_manager.write().await.set_active_ssh(Some(uri.clone()));

            (
                StatusCode::OK,
                Json(serde_json::json!({
                    "success": true,
                    "uri": uri
                })),
            )
        }
        Err(e) => (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": e.to_string() })),
        ),
    }
}

async fn disconnect_ssh(State(state): State<AppState>) -> impl IntoResponse {
    state.session_manager.write().await.set_active_ssh(None);

    (
        StatusCode::OK,
        Json(serde_json::json!({ "success": true })),
    )
}

// Share link handlers

async fn create_share_link(
    Path(session_id): Path<String>,
    Json(options): Json<crate::sharing::ShareOptions>,
) -> impl IntoResponse {
    let link = crate::sharing::create_share_link(&session_id, options);
    (
        StatusCode::OK,
        Json(serde_json::json!({
            "token": link.token,
            "session_id": link.session_id,
            "max_uses": link.max_uses,
            "read_only": link.read_only,
            "url": format!("/?share={}", link.token)
        })),
    )
}

async fn list_share_links(Path(session_id): Path<String>) -> impl IntoResponse {
    let links = crate::sharing::list_share_links(&session_id);
    Json(serde_json::json!({ "links": links }))
}

async fn use_share_link(Path(token): Path<String>) -> impl IntoResponse {
    match crate::sharing::use_share_link(&token) {
        Some((session_id, read_only)) => (
            StatusCode::OK,
            Json(serde_json::json!({ "session_id": session_id, "read_only": read_only })),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({ "error": "share link invalid or expired" })),
        ),
    }
}

async fn revoke_share_link(Path(token): Path<String>) -> impl IntoResponse {
    if crate::sharing::revoke_share_link(&token) {
        Json(serde_json::json!({ "revoked": true }))
    } else {
        Json(serde_json::json!({ "error": "share link not found" }))
    }
}

// Snapshot handlers

#[derive(Deserialize)]
struct SnapshotRequest {
    cwd: Option<String>,
    open_files: Option<Vec<String>>,
    current_file: Option<String>,
    cursor: Option<(String, u32, u32)>,
    description: Option<String>,
}

async fn create_snapshot(
    Path(session_id): Path<String>,
    Json(payload): Json<SnapshotRequest>,
) -> impl IntoResponse {
    let snap = crate::sharing::create_snapshot(
        &session_id,
        std::path::PathBuf::from(payload.cwd.unwrap_or_default()),
        payload.open_files.unwrap_or_default(),
        payload.current_file,
        payload.cursor,
        payload.description,
    );
    Json(serde_json::json!({ "id": snap.id, "session_id": snap.session_id }))
}

async fn list_snapshots(Path(session_id): Path<String>) -> impl IntoResponse {
    let snapshots = crate::sharing::list_snapshots(&session_id);
    Json(serde_json::json!({ "snapshots": snapshots }))
}

async fn get_snapshot(Path(id): Path<String>) -> impl IntoResponse {
    match crate::sharing::get_snapshot(&id) {
        Some(snap) => (StatusCode::OK, Json(serde_json::to_value(snap).unwrap())),
        None => (StatusCode::NOT_FOUND, Json(serde_json::json!({ "error": "not found" }))),
    }
}

async fn delete_snapshot_handler(Path(id): Path<String>) -> impl IntoResponse {
    if crate::sharing::delete_snapshot(&id) {
        Json(serde_json::json!({ "deleted": true }))
    } else {
        Json(serde_json::json!({ "error": "not found" }))
    }
}

// Deprecated entry point kept for signature compatibility if needed, but unused
pub fn serve_api(
    _addr: &str,
    _session_manager: Arc<RwLock<AsyncSessionManager>>,
) -> anyhow::Result<()> {
    Ok(())
}
