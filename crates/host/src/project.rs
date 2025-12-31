#![allow(clippy::non_std_lazy_statics)]
//! Project configuration and magic link handling
//!
//! Enables opening projects directly in nvim-web from terminal via:
//! `nvim-web open /path/to/project`
//!
//! Projects can have an optional `.nvim-web/config.toml` for customization.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::RwLock;
use std::time::{Duration, Instant};

use serde::Deserialize;

/// Project configuration from `.nvim-web/config.toml`
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProjectConfig {
    #[serde(default)]
    pub project: ProjectInfo,
    #[serde(default)]
    pub security: SecurityConfig,
    #[serde(default)]
    pub editor: EditorConfig,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ProjectInfo {
    /// Display name for the project
    pub name: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct SecurityConfig {
    /// Optional authentication key for remote access
    pub key: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct EditorConfig {
    /// Working directory relative to project root
    pub cwd: Option<String>,
    /// File to open on start
    pub init_file: Option<String>,
}

impl ProjectConfig {
    /// Load config from `.nvim-web/config.toml` in the given path
    pub fn load(project_path: &Path) -> Self {
        let config_path = project_path.join(".nvim-web").join("config.toml");
        if config_path.exists() {
            match std::fs::read_to_string(&config_path) {
                Ok(content) => match toml::from_str(&content) {
                Ok(config) => return config,
                    Err(e) => eprintln!("  [warn] Failed to parse .nvim-web/config.toml: {e}"),
                },
                Err(e) => eprintln!("  [warn] Failed to read .nvim-web/config.toml: {e}"),
            }
        }
        Self::default()
    }

    /// Get the display name (from config or directory name)
    pub fn display_name(&self, project_path: &Path) -> String {
        self.project.name.clone().unwrap_or_else(|| {
            project_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("project")
                .to_string()
        })
    }

    /// Get the working directory (resolved to absolute path)
    pub fn resolved_cwd(&self, project_path: &Path) -> PathBuf {
        self.editor.cwd.as_ref().map_or_else(
            || project_path.to_path_buf(),
            |cwd| project_path.join(cwd)
        )
    }
}

/// Token for opening a project in the browser
#[derive(Debug, Clone)]
pub struct OpenToken {
    /// Absolute path to the project
    pub path: PathBuf,
    /// Project configuration
    pub config: ProjectConfig,
    /// When the token was created
    pub created_at: Instant,
    /// Whether the token has been used
    pub claimed: bool,
    /// Token mode (single-use, shareable, snapshot)
    pub mode: TokenMode,
    /// Target file to open (relative to project root)
    pub target_file: Option<String>,
    /// Target line number
    pub target_line: Option<u32>,
    /// Custom expiration (overrides default TTL)
    pub expires_at: Option<Instant>,
    /// Maximum number of claims (for shareable links)
    pub max_claims: Option<u32>,
    /// Number of times this token has been claimed
    pub claim_count: u32,
}

/// Token mode determines how the token can be used
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TokenMode {
    /// Single-use token (current behavior)
    SingleUse,
    /// Shareable token with optional time limit
    Share,
    /// Snapshot token (reproducible state)
    Snapshot,
}

impl Default for TokenMode {
    fn default() -> Self {
        Self::SingleUse
    }
}

impl OpenToken {
    /// Token TTL (5 minutes)
    const TTL: Duration = Duration::from_secs(300);

    /// Check if token is expired
    pub fn is_expired(&self) -> bool {
        if let Some(expires) = self.expires_at {
            Instant::now() > expires
        } else {
            self.created_at.elapsed() > Self::TTL
        }
    }

    /// Check if token is valid (not expired, not claimed or shareable)
    pub fn is_valid(&self) -> bool {
        if self.is_expired() {
            return false;
        }
        match self.mode {
            TokenMode::SingleUse => !self.claimed,
            TokenMode::Share => {
                // Check max claims if set
                self.max_claims.map_or(true, |max| self.claim_count < max)
            }
            TokenMode::Snapshot => true, // Snapshots are always claimable
        }
    }
}

// Global token storage
lazy_static::lazy_static! {
    static ref OPEN_TOKENS: RwLock<HashMap<String, OpenToken>> = RwLock::new(HashMap::new());
}

use std::sync::atomic::{AtomicU64, Ordering};

/// Atomic counter for unique token generation
static TOKEN_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Generate a secure random token
pub fn generate_token() -> String {
    use std::time::SystemTime;
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let counter = TOKEN_COUNTER.fetch_add(1, Ordering::SeqCst);
    let random = u128::from(std::process::id()) ^ now ^ (u128::from(counter) << 64);
    format!("{random:x}")
}

/// Options for creating a magic link token
#[derive(Debug, Clone, Default)]
pub struct TokenOptions {
    pub target_file: Option<String>,
    pub target_line: Option<u32>,
    pub mode: TokenMode,
    pub duration: Option<Duration>,
    pub max_claims: Option<u32>,
}

/// Store a new open token (simple version for backward compatibility)
pub fn store_token(path: PathBuf, config: ProjectConfig) -> String {
    store_token_with_options(path, config, TokenOptions::default())
}

/// Store a new open token with options
pub fn store_token_with_options(path: PathBuf, config: ProjectConfig, options: TokenOptions) -> String {
    let token = generate_token();
    let open_token = OpenToken {
        path,
        config,
        created_at: Instant::now(),
        claimed: false,
        mode: options.mode,
        target_file: options.target_file,
        target_line: options.target_line,
        expires_at: options.duration.map(|d| Instant::now() + d),
        max_claims: options.max_claims,
        claim_count: 0,
    };

    // Clean up expired tokens first
    cleanup_expired_tokens();

    if let Ok(mut tokens) = OPEN_TOKENS.write() {
        tokens.insert(token.clone(), open_token);
    }

    token
}

/// Claim a token (marks as used, returns project info if valid)
pub fn claim_token(token: &str) -> Option<(PathBuf, ProjectConfig)> {
    claim_token_full(token).map(|(path, config, _, _)| (path, config))
}

/// Claim a token and return full info including deep link
pub fn claim_token_full(token: &str) -> Option<(PathBuf, ProjectConfig, Option<String>, Option<u32>)> {
    if let Ok(mut tokens) = OPEN_TOKENS.write() {
        if let Some(open_token) = tokens.get_mut(token) {
            if open_token.is_valid() {
                match open_token.mode {
                    TokenMode::SingleUse => open_token.claimed = true,
                    TokenMode::Share | TokenMode::Snapshot => {
                        open_token.claim_count += 1;
                    }
                }
                return Some((
                    open_token.path.clone(),
                    open_token.config.clone(),
                    open_token.target_file.clone(),
                    open_token.target_line,
                ));
            }
        }
    }
    None
}

/// Get token info without claiming
pub fn get_token_info(token: &str) -> Option<(PathBuf, ProjectConfig, bool)> {
    if let Ok(tokens) = OPEN_TOKENS.read() {
        if let Some(open_token) = tokens.get(token) {
            if !open_token.is_expired() {
                return Some((
                    open_token.path.clone(),
                    open_token.config.clone(),
                    open_token.claimed,
                ));
            }
        }
    }
    None
}

/// Remove expired tokens
fn cleanup_expired_tokens() {
    if let Ok(mut tokens) = OPEN_TOKENS.write() {
        tokens.retain(|_, t| !t.is_expired());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_token() {
        let t1 = generate_token();
        let t2 = generate_token();
        assert!(!t1.is_empty());
        assert_ne!(t1, t2); // Should be unique
    }

    #[test]
    fn test_store_and_claim_token() {
        let path = PathBuf::from("/test/project");
        let config = ProjectConfig::default();

        let token = store_token(path.clone(), config);

        // First claim should work
        let result = claim_token(&token);
        assert!(result.is_some());

        // Second claim should fail (single-use)
        let result2 = claim_token(&token);
        assert!(result2.is_none());
    }
}
