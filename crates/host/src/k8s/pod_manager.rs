//! Pod lifecycle manager
//!
//! Handles creation, deletion, and monitoring of session pods.

use anyhow::Result;
use k8s_openapi::api::core::v1::Pod;
use kube::{
    api::{Api, DeleteParams, ListParams, PostParams},
    Client,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{info, warn};

use super::{K8sConfig, PodStatus, SessionPod, SessionPodSpec};

/// Pod manager for session lifecycle
pub struct PodManager {
    /// Kubernetes client
    client: Client,
    /// Configuration
    config: K8sConfig,
    /// Active sessions (session_id -> SessionPod)
    sessions: Arc<RwLock<HashMap<String, SessionPod>>>,
}

impl PodManager {
    /// Create a new pod manager
    pub async fn new(config: K8sConfig) -> Result<Self> {
        let client = Client::try_default().await?;

        Ok(Self {
            client,
            config,
            sessions: Arc::new(RwLock::new(HashMap::new())),
        })
    }

    /// Create a new session pod
    pub async fn create_session(&self, session_id: String) -> Result<SessionPod> {
        // Check max sessions limit
        {
            let sessions = self.sessions.read().await;
            if sessions.len() >= self.config.max_sessions {
                anyhow::bail!("Maximum session limit reached");
            }
        }

        // Build pod spec
        let spec = SessionPodSpec::new(session_id.clone(), self.config.clone());
        let pod = spec.build_pod();
        let pod_name = spec.pod_name();

        // Create pod
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.config.namespace);
        let _created = pods.create(&PostParams::default(), &pod).await?;

        info!(session_id = %session_id, pod_name = %pod_name, "Created session pod");

        // Build session pod record
        let session_pod = SessionPod {
            session_id: session_id.clone(),
            pod_name: pod_name.clone(),
            pod_ip: None,
            status: PodStatus::Pending,
            created_at: chrono::Utc::now(),
        };

        // Store in sessions map
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(session_id, session_pod.clone());
        }

        Ok(session_pod)
    }

    /// Delete a session pod
    pub async fn delete_session(&self, session_id: &str) -> Result<()> {
        let pod_name = {
            let sessions = self.sessions.read().await;
            sessions.get(session_id).map(|s| s.pod_name.clone())
        };

        if let Some(pod_name) = pod_name {
            let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.config.namespace);
            pods.delete(&pod_name, &DeleteParams::default()).await?;

            info!(session_id = %session_id, pod_name = %pod_name, "Deleted session pod");

            // Remove from sessions map
            let mut sessions = self.sessions.write().await;
            sessions.remove(session_id);
        }

        Ok(())
    }

    /// Get a session pod
    pub async fn get_session(&self, session_id: &str) -> Option<SessionPod> {
        let sessions = self.sessions.read().await;
        sessions.get(session_id).cloned()
    }

    /// List all active sessions
    pub async fn list_sessions(&self) -> Vec<SessionPod> {
        let sessions = self.sessions.read().await;
        sessions.values().cloned().collect()
    }

    /// Refresh session status from Kubernetes
    pub async fn refresh_session(&self, session_id: &str) -> Result<Option<SessionPod>> {
        let pod_name = {
            let sessions = self.sessions.read().await;
            sessions.get(session_id).map(|s| s.pod_name.clone())
        };

        if let Some(pod_name) = pod_name {
            let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.config.namespace);

            match pods.get(&pod_name).await {
                Ok(pod) => {
                    let status = parse_pod_status(&pod);
                    let pod_ip = pod.status.as_ref().and_then(|s| s.pod_ip.clone());

                    // Update session
                    let mut sessions = self.sessions.write().await;
                    if let Some(session) = sessions.get_mut(session_id) {
                        session.status = status;
                        session.pod_ip = pod_ip;
                        return Ok(Some(session.clone()));
                    }
                }
                Err(kube::Error::Api(e)) if e.code == 404 => {
                    // Pod was deleted externally
                    let mut sessions = self.sessions.write().await;
                    sessions.remove(session_id);
                    return Ok(None);
                }
                Err(e) => return Err(e.into()),
            }
        }

        Ok(None)
    }

    /// Cleanup expired sessions
    pub async fn cleanup_expired(&self) -> Result<usize> {
        let now = chrono::Utc::now();
        let timeout = chrono::Duration::seconds(self.config.session_timeout_secs as i64);

        let expired: Vec<String> = {
            let sessions = self.sessions.read().await;
            sessions
                .iter()
                .filter(|(_, s)| now - s.created_at > timeout)
                .map(|(id, _)| id.clone())
                .collect()
        };

        let count = expired.len();
        for session_id in expired {
            if let Err(e) = self.delete_session(&session_id).await {
                warn!(session_id = %session_id, error = %e, "Failed to cleanup expired session");
            }
        }

        if count > 0 {
            info!(count = count, "Cleaned up expired sessions");
        }

        Ok(count)
    }

    /// Sync sessions with Kubernetes
    pub async fn sync_from_k8s(&self) -> Result<()> {
        let pods: Api<Pod> = Api::namespaced(self.client.clone(), &self.config.namespace);

        let lp = ListParams::default().labels("app=nvim-web-session");
        let pod_list = pods.list(&lp).await?;

        let mut sessions = self.sessions.write().await;

        for pod in pod_list {
            let session_id = pod
                .metadata
                .annotations
                .as_ref()
                .and_then(|a| a.get("nvim-web/session-id"))
                .cloned();

            if let Some(session_id) = session_id {
                let pod_name = pod.metadata.name.clone().unwrap_or_default();
                let pod_ip = pod.status.as_ref().and_then(|s| s.pod_ip.clone());
                let status = parse_pod_status(&pod);

                sessions.insert(
                    session_id.clone(),
                    SessionPod {
                        session_id,
                        pod_name,
                        pod_ip,
                        status,
                        created_at: chrono::Utc::now(), // Approximate
                    },
                );
            }
        }

        Ok(())
    }
}

/// Parse pod phase to status
fn parse_pod_status(pod: &Pod) -> PodStatus {
    pod.status
        .as_ref()
        .and_then(|s| s.phase.as_ref())
        .map(|p| match p.as_str() {
            "Pending" => PodStatus::Pending,
            "Running" => PodStatus::Running,
            "Succeeded" => PodStatus::Succeeded,
            "Failed" => PodStatus::Failed,
            _ => PodStatus::Unknown,
        })
        .unwrap_or(PodStatus::Unknown)
}
