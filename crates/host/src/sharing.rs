//! Session sharing and workspace snapshots
//!
//! Provides:
//! - Share link generation with expiry and use limits
//! - Workspace snapshots for reproducible state
//! - API endpoints for sharing

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::RwLock;
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};

/// Share link configuration
#[derive(Debug, Clone, Serialize)]
pub struct ShareLink {
    /// Unique share token
    pub token: String,
    /// Session ID to share
    pub session_id: String,
    /// When the link was created
    pub created_at: SystemTime,
    /// When the link expires (None = never)
    pub expires_at: Option<SystemTime>,
    /// Maximum number of uses (None = unlimited)
    pub max_uses: Option<u32>,
    /// Current use count
    pub use_count: u32,
    /// Read-only sharing
    pub read_only: bool,
    /// Optional label for the link
    pub label: Option<String>,
}

/// Options for creating a share link
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ShareOptions {
    /// Duration until expiry (seconds)
    pub ttl_secs: Option<u64>,
    /// Maximum number of uses
    pub max_uses: Option<u32>,
    /// Read-only access
    #[serde(default = "default_readonly")]
    pub read_only: bool,
    /// Optional label
    pub label: Option<String>,
}

fn default_readonly() -> bool {
    true
}

impl ShareLink {
    /// Check if the link is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires_at) = self.expires_at {
            SystemTime::now() >= expires_at
        } else {
            false
        }
    }

    /// Check if the link has remaining uses
    pub fn has_uses_remaining(&self) -> bool {
        if let Some(max) = self.max_uses {
            self.use_count < max
        } else {
            true
        }
    }

    /// Check if the link is valid (not expired and has uses)
    pub fn is_valid(&self) -> bool {
        !self.is_expired() && self.has_uses_remaining()
    }

    /// Get remaining time until expiry
    pub fn time_remaining(&self) -> Option<Duration> {
        self.expires_at
            .and_then(|exp| exp.duration_since(SystemTime::now()).ok())
    }
}

/// Workspace snapshot - captures session state at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Snapshot {
    /// Unique snapshot ID
    pub id: String,
    /// Session this snapshot was taken from
    pub session_id: String,
    /// When the snapshot was taken
    pub created_at: SystemTime,
    /// Working directory path
    pub cwd: PathBuf,
    /// List of open files
    pub open_files: Vec<String>,
    /// Current file (focused)
    pub current_file: Option<String>,
    /// Cursor position (file, line, col)
    pub cursor: Option<(String, u32, u32)>,
    /// Optional description
    pub description: Option<String>,
}

// Share link storage
lazy_static::lazy_static! {
    static ref SHARE_LINKS: RwLock<HashMap<String, ShareLink>> = RwLock::new(HashMap::new());
    static ref SNAPSHOTS: RwLock<HashMap<String, Snapshot>> = RwLock::new(HashMap::new());
}

/// Generate a unique share token
fn generate_share_token() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros();

    // Create a short, URL-safe token
    let raw = format!("{timestamp:x}{count:x}");
    // Take last 12 characters for brevity
    raw.chars()
        .rev()
        .take(12)
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

/// Generate a snapshot ID
fn generate_snapshot_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let count = COUNTER.fetch_add(1, Ordering::SeqCst);
    let timestamp = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    format!("snap_{timestamp:x}_{count}")
}

/// Create a share link for a session
pub fn create_share_link(session_id: &str, options: ShareOptions) -> ShareLink {
    let token = generate_share_token();
    let now = SystemTime::now();

    let link = ShareLink {
        token: token.clone(),
        session_id: session_id.to_string(),
        created_at: now,
        expires_at: options.ttl_secs.map(|secs| now + Duration::from_secs(secs)),
        max_uses: options.max_uses,
        use_count: 0,
        read_only: options.read_only,
        label: options.label,
    };

    if let Ok(mut links) = SHARE_LINKS.write() {
        links.insert(token.clone(), link.clone());
    }

    link
}

/// Validate and use a share link
pub fn use_share_link(token: &str) -> Option<(String, bool)> {
    let mut links = SHARE_LINKS.write().ok()?;
    let link = links.get_mut(token)?;

    if !link.is_valid() {
        return None;
    }

    link.use_count += 1;
    Some((link.session_id.clone(), link.read_only))
}

/// Get share link info without using it
pub fn get_share_link(token: &str) -> Option<ShareLink> {
    let links = SHARE_LINKS.read().ok()?;
    links.get(token).cloned()
}

/// List all share links for a session
pub fn list_share_links(session_id: &str) -> Vec<ShareLink> {
    let links = SHARE_LINKS.read().ok();
    links
        .map(|l| {
            l.values()
                .filter(|link| link.session_id == session_id && link.is_valid())
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// Revoke a share link
pub fn revoke_share_link(token: &str) -> bool {
    if let Ok(mut links) = SHARE_LINKS.write() {
        links.remove(token).is_some()
    } else {
        false
    }
}

/// Create a workspace snapshot
pub fn create_snapshot(
    session_id: &str,
    cwd: PathBuf,
    open_files: Vec<String>,
    current_file: Option<String>,
    cursor: Option<(String, u32, u32)>,
    description: Option<String>,
) -> Snapshot {
    let id = generate_snapshot_id();

    let snapshot = Snapshot {
        id: id.clone(),
        session_id: session_id.to_string(),
        created_at: SystemTime::now(),
        cwd,
        open_files,
        current_file,
        cursor,
        description,
    };

    if let Ok(mut snapshots) = SNAPSHOTS.write() {
        snapshots.insert(id, snapshot.clone());
    }

    snapshot
}

/// Get a snapshot by ID
pub fn get_snapshot(id: &str) -> Option<Snapshot> {
    let snapshots = SNAPSHOTS.read().ok()?;
    snapshots.get(id).cloned()
}

/// List snapshots for a session
pub fn list_snapshots(session_id: &str) -> Vec<Snapshot> {
    let snapshots = SNAPSHOTS.read().ok();
    snapshots
        .map(|s| {
            s.values()
                .filter(|snap| snap.session_id == session_id)
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// Delete a snapshot
pub fn delete_snapshot(id: &str) -> bool {
    if let Ok(mut snapshots) = SNAPSHOTS.write() {
        snapshots.remove(id).is_some()
    } else {
        false
    }
}

/// Clean up expired share links
pub fn cleanup_expired() {
    if let Ok(mut links) = SHARE_LINKS.write() {
        links.retain(|_, link| link.is_valid());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_share_link_creation() {
        let link = create_share_link(
            "test-session",
            ShareOptions {
                ttl_secs: Some(3600),
                max_uses: Some(5),
                read_only: true,
                label: Some("Team share".to_string()),
            },
        );

        assert_eq!(link.session_id, "test-session");
        assert!(link.is_valid());
        assert_eq!(link.max_uses, Some(5));
        assert!(link.expires_at.is_some());
    }

    #[test]
    fn test_share_link_use() {
        let link = create_share_link(
            "session-1",
            ShareOptions {
                max_uses: Some(2),
                ..Default::default()
            },
        );

        // First use
        let result = use_share_link(&link.token);
        assert!(result.is_some());

        // Second use
        let result = use_share_link(&link.token);
        assert!(result.is_some());

        // Third use should fail (max_uses = 2)
        let result = use_share_link(&link.token);
        assert!(result.is_none());
    }

    #[test]
    fn test_snapshot_creation() {
        let snap = create_snapshot(
            "session-1",
            PathBuf::from("/project"),
            vec!["main.rs".to_string(), "lib.rs".to_string()],
            Some("main.rs".to_string()),
            Some(("main.rs".to_string(), 10, 5)),
            Some("Before refactor".to_string()),
        );

        assert!(!snap.id.is_empty());
        assert_eq!(snap.session_id, "session-1");
        assert_eq!(snap.open_files.len(), 2);

        // Retrieve it
        let retrieved = get_snapshot(&snap.id);
        assert!(retrieved.is_some());
    }
}
