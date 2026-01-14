//! Session router for pod-per-session mode
//!
//! Provides route configuration for session management in Kubernetes mode.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};

use super::SessionPod;

/// Create session request
#[derive(Debug, Clone, Deserialize)]
pub struct CreateSessionRequest {
    #[serde(default)]
    pub user_id: Option<String>,
}

/// Connection info response
#[derive(Debug, Clone, Serialize)]
pub struct ConnectInfo {
    pub session_id: String,
    pub pod_ip: Option<String>,
    pub ws_url: Option<String>,
    pub ready: bool,
}

impl From<SessionPod> for ConnectInfo {
    fn from(session: SessionPod) -> Self {
        let ready = session.is_ready();
        let ws_url = session
            .pod_ip
            .as_ref()
            .map(|ip| format!("ws://{}:9001", ip));
        Self {
            session_id: session.session_id,
            pod_ip: session.pod_ip,
            ws_url,
            ready,
        }
    }
}

/// Session list response
#[derive(Debug, Clone, Serialize)]
pub struct SessionListResponse {
    pub sessions: Vec<SessionPod>,
    pub count: usize,
}

impl From<Vec<SessionPod>> for SessionListResponse {
    fn from(sessions: Vec<SessionPod>) -> Self {
        let count = sessions.len();
        Self { sessions, count }
    }
}

/// Session router configuration helper
///
/// Instead of providing axum routes directly (which require complex type handling),
/// this provides types and conversions for building K8s session management routes.
///
/// Example integration:
/// ```ignore
/// use nvim_web_host::k8s::{PodManager, ConnectInfo, CreateSessionRequest};
///
/// async fn create_session(manager: &PodManager, req: CreateSessionRequest) -> Result<SessionPod, Error> {
///     let session_id = uuid::Uuid::new_v4().to_string();
///     manager.create_session(session_id).await
/// }
///
/// async fn get_connect_info(manager: &PodManager, id: &str) -> Option<ConnectInfo> {
///     manager.get_session(id).await.map(ConnectInfo::from)
/// }
/// ```
pub struct SessionRouter;

impl SessionRouter {
    /// Health check response
    pub fn health_check_response() -> &'static str {
        "OK"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::k8s::{PodStatus, SessionPod};

    #[test]
    fn test_connect_info_from_session() {
        let session = SessionPod {
            session_id: "test-123".to_string(),
            pod_name: "nvim-web-session-test-123".to_string(),
            pod_ip: Some("10.0.0.1".to_string()),
            status: PodStatus::Running,
            created_at: chrono::Utc::now(),
        };

        let info = ConnectInfo::from(session);
        assert_eq!(info.session_id, "test-123");
        assert!(info.ready);
        assert_eq!(info.ws_url, Some("ws://10.0.0.1:9001".to_string()));
    }

    #[test]
    fn test_session_list_response() {
        let sessions = vec![];
        let response = SessionListResponse::from(sessions);
        assert_eq!(response.count, 0);
    }
}
