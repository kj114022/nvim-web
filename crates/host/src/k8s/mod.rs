//! Kubernetes integration for pod-per-session scaling
//!
//! Provides infrastructure for running each nvim-web session
//! in an isolated Kubernetes pod with:
//! - Dynamic pod creation/deletion
//! - Session routing
//! - Persistent storage (PVC)

mod pod_manager;
mod router;
mod session_pod;

pub use pod_manager::PodManager;
pub use router::SessionRouter;
pub use session_pod::{PodStatus, SessionPod, SessionPodSpec};

use serde::{Deserialize, Serialize};

/// Kubernetes configuration for pod-per-session mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct K8sConfig {
    /// Enable Kubernetes mode
    #[serde(default)]
    pub enabled: bool,

    /// Kubernetes namespace for session pods
    #[serde(default = "default_namespace")]
    pub namespace: String,

    /// Session pod image
    #[serde(default = "default_image")]
    pub image: String,

    /// Pod resource limits
    #[serde(default)]
    pub resources: PodResources,

    /// Storage class for session PVCs
    #[serde(default)]
    pub storage_class: Option<String>,

    /// Storage size for session PVCs
    #[serde(default = "default_storage_size")]
    pub storage_size: String,

    /// Session timeout in seconds
    #[serde(default = "default_session_timeout")]
    pub session_timeout_secs: u64,

    /// Maximum concurrent sessions
    #[serde(default = "default_max_sessions")]
    pub max_sessions: usize,

    /// Labels to apply to session pods
    #[serde(default)]
    pub labels: std::collections::HashMap<String, String>,
}

fn default_namespace() -> String {
    "nvim-web".to_string()
}

fn default_image() -> String {
    "ghcr.io/nvim-web/session:latest".to_string()
}

fn default_storage_size() -> String {
    "1Gi".to_string()
}

fn default_session_timeout() -> u64 {
    3600 // 1 hour
}

fn default_max_sessions() -> usize {
    100
}

/// Pod resource configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PodResources {
    /// CPU request
    #[serde(default = "default_cpu_request")]
    pub cpu_request: String,

    /// CPU limit
    #[serde(default = "default_cpu_limit")]
    pub cpu_limit: String,

    /// Memory request
    #[serde(default = "default_memory_request")]
    pub memory_request: String,

    /// Memory limit
    #[serde(default = "default_memory_limit")]
    pub memory_limit: String,
}

fn default_cpu_request() -> String {
    "100m".to_string()
}

fn default_cpu_limit() -> String {
    "1".to_string()
}

fn default_memory_request() -> String {
    "128Mi".to_string()
}

fn default_memory_limit() -> String {
    "512Mi".to_string()
}

impl Default for PodResources {
    fn default() -> Self {
        Self {
            cpu_request: default_cpu_request(),
            cpu_limit: default_cpu_limit(),
            memory_request: default_memory_request(),
            memory_limit: default_memory_limit(),
        }
    }
}

impl Default for K8sConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            namespace: default_namespace(),
            image: default_image(),
            resources: PodResources::default(),
            storage_class: None,
            storage_size: default_storage_size(),
            session_timeout_secs: default_session_timeout(),
            max_sessions: default_max_sessions(),
            labels: std::collections::HashMap::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = K8sConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.namespace, "nvim-web");
        assert_eq!(config.max_sessions, 100);
    }

    #[test]
    fn test_default_resources() {
        let resources = PodResources::default();
        assert_eq!(resources.cpu_request, "100m");
        assert_eq!(resources.memory_limit, "512Mi");
    }
}
