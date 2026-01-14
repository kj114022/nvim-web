//! Session pod definition
//!
//! Defines the Kubernetes Pod spec for session containers.

use k8s_openapi::api::core::v1::{
    Container, EnvVar, Pod, PodSpec, ResourceRequirements, VolumeMount,
};
use k8s_openapi::apimachinery::pkg::api::resource::Quantity;
use kube::api::ObjectMeta;
use std::collections::BTreeMap;

use super::{K8sConfig, PodResources};

/// Session pod specification
#[derive(Debug, Clone)]
pub struct SessionPodSpec {
    /// Session ID
    pub session_id: String,
    /// User ID (from auth)
    pub user_id: Option<String>,
    /// Pod configuration
    pub config: K8sConfig,
}

/// Active session pod
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SessionPod {
    /// Session ID
    pub session_id: String,
    /// Pod name
    pub pod_name: String,
    /// Pod IP address
    pub pod_ip: Option<String>,
    /// Pod status
    pub status: PodStatus,
    /// Creation timestamp
    pub created_at: chrono::DateTime<chrono::Utc>,
}

/// Pod status
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum PodStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Unknown,
}

impl SessionPodSpec {
    /// Create a new session pod spec
    pub fn new(session_id: String, config: K8sConfig) -> Self {
        Self {
            session_id,
            user_id: None,
            config,
        }
    }

    /// Set user ID
    pub fn with_user(mut self, user_id: String) -> Self {
        self.user_id = Some(user_id);
        self
    }

    /// Generate pod name
    pub fn pod_name(&self) -> String {
        format!(
            "nvim-web-session-{}",
            &self.session_id[..8.min(self.session_id.len())]
        )
    }

    /// Build Kubernetes Pod object
    pub fn build_pod(&self) -> Pod {
        let pod_name = self.pod_name();

        // Labels
        let mut labels: BTreeMap<String, String> = self
            .config
            .labels
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        labels.insert("app".to_string(), "nvim-web-session".to_string());
        labels.insert("session-id".to_string(), self.session_id.clone());
        if let Some(ref user_id) = self.user_id {
            labels.insert("user-id".to_string(), user_id.clone());
        }

        // Annotations
        let mut annotations = BTreeMap::new();
        annotations.insert("nvim-web/session-id".to_string(), self.session_id.clone());

        // Environment variables
        let env_vars = vec![
            EnvVar {
                name: "SESSION_ID".to_string(),
                value: Some(self.session_id.clone()),
                ..Default::default()
            },
            EnvVar {
                name: "NVIM_WEB_MODE".to_string(),
                value: Some("session".to_string()),
                ..Default::default()
            },
        ];

        // Resource requirements
        let resources = build_resources(&self.config.resources);

        // Container
        let container = Container {
            name: "nvim-web".to_string(),
            image: Some(self.config.image.clone()),
            env: Some(env_vars),
            resources: Some(resources),
            ports: Some(vec![k8s_openapi::api::core::v1::ContainerPort {
                container_port: 9001,
                name: Some("ws".to_string()),
                protocol: Some("TCP".to_string()),
                ..Default::default()
            }]),
            volume_mounts: Some(vec![VolumeMount {
                name: "workspace".to_string(),
                mount_path: "/workspace".to_string(),
                ..Default::default()
            }]),
            ..Default::default()
        };

        // Pod spec
        let spec = PodSpec {
            containers: vec![container],
            restart_policy: Some("Never".to_string()),
            ..Default::default()
        };

        Pod {
            metadata: ObjectMeta {
                name: Some(pod_name),
                namespace: Some(self.config.namespace.clone()),
                labels: Some(labels),
                annotations: Some(annotations),
                ..Default::default()
            },
            spec: Some(spec),
            ..Default::default()
        }
    }
}

/// Build resource requirements from config
fn build_resources(config: &PodResources) -> ResourceRequirements {
    let mut requests = BTreeMap::new();
    requests.insert("cpu".to_string(), Quantity(config.cpu_request.clone()));
    requests.insert(
        "memory".to_string(),
        Quantity(config.memory_request.clone()),
    );

    let mut limits = BTreeMap::new();
    limits.insert("cpu".to_string(), Quantity(config.cpu_limit.clone()));
    limits.insert("memory".to_string(), Quantity(config.memory_limit.clone()));

    ResourceRequirements {
        requests: Some(requests),
        limits: Some(limits),
        ..Default::default()
    }
}

impl SessionPod {
    /// Check if pod is ready for connections
    pub fn is_ready(&self) -> bool {
        self.status == PodStatus::Running && self.pod_ip.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pod_name_generation() {
        let spec = SessionPodSpec::new("abc123def456".to_string(), K8sConfig::default());
        assert_eq!(spec.pod_name(), "nvim-web-session-abc123de");
    }

    #[test]
    fn test_build_pod() {
        let spec = SessionPodSpec::new("test-session".to_string(), K8sConfig::default());
        let pod = spec.build_pod();

        assert_eq!(
            pod.metadata.name,
            Some("nvim-web-session-test-ses".to_string())
        );
        assert_eq!(pod.metadata.namespace, Some("nvim-web".to_string()));
        assert!(pod.spec.is_some());
    }
}
