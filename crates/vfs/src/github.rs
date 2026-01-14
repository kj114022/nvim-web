//! GitHub VFS backend
//!
//! Provides read/write access to GitHub repositories via the GitHub API.
//! Enables clone-less editing of remote repositories.
//!
//! URI format: `vfs://github/owner/repo/path/to/file.rs`
//! With branch: `vfs://github/owner/repo@branch/path/to/file.rs`

use anyhow::{bail, Result};
use async_trait::async_trait;

use super::{FileStat, VfsBackend};

/// GitHub VFS backend for remote repository access
pub struct GitHubFsBackend {
    /// Octocrab client
    client: octocrab::Octocrab,
    /// Optional auth token for private repos
    _token: Option<String>,
}

/// Parsed GitHub path components
#[derive(Debug, Clone)]
struct GitHubPath {
    owner: String,
    repo: String,
    branch: Option<String>,
    path: String,
}

impl GitHubFsBackend {
    /// Create a new GitHub backend (unauthenticated - public repos only)
    pub fn new() -> Self {
        Self {
            client: octocrab::Octocrab::default(),
            _token: None,
        }
    }

    /// Create with authentication token (for private repos)
    pub fn with_token(token: impl Into<String>) -> Result<Self> {
        let token_str = token.into();
        let client = octocrab::Octocrab::builder()
            .personal_token(token_str.clone())
            .build()
            .map_err(|e| anyhow::anyhow!("Failed to create GitHub client: {e}"))?;

        Ok(Self {
            client,
            _token: Some(token_str),
        })
    }

    /// Create from environment variable GITHUB_TOKEN
    pub fn from_env() -> Result<Self> {
        if let Ok(token) = std::env::var("GITHUB_TOKEN") {
            Self::with_token(token)
        } else {
            Ok(Self::new())
        }
    }

    /// Parse a GitHub path into components
    /// Format: owner/repo/path or owner/repo@branch/path
    fn parse_path(path: &str) -> Result<GitHubPath> {
        let path = path.trim_start_matches('/');
        let parts: Vec<&str> = path.splitn(3, '/').collect();

        if parts.len() < 2 {
            bail!("Invalid GitHub path: {path}. Expected: owner/repo/path");
        }

        let owner = parts[0].to_string();

        // Check for branch specification: repo@branch
        let (repo, branch) = if let Some((r, b)) = parts[1].split_once('@') {
            (r.to_string(), Some(b.to_string()))
        } else {
            (parts[1].to_string(), None)
        };

        let file_path = if parts.len() > 2 {
            parts[2].to_string()
        } else {
            String::new()
        };

        Ok(GitHubPath {
            owner,
            repo,
            branch,
            path: file_path,
        })
    }
}

impl Default for GitHubFsBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VfsBackend for GitHubFsBackend {
    async fn read(&self, path: &str) -> Result<Vec<u8>> {
        let gh = Self::parse_path(path)?;

        // Use Contents API to get file content
        let content = self
            .client
            .repos(&gh.owner, &gh.repo)
            .get_content()
            .path(&gh.path)
            .r#ref(gh.branch.as_deref().unwrap_or("main"))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("GitHub API error: {e}"))?;

        // Extract file content from response
        match content.items.first() {
            Some(item) => {
                if let Some(ref encoded) = item.content {
                    // Content is base64 encoded with newlines
                    let clean = encoded.replace('\n', "");
                    let decoded =
                        base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &clean)
                            .map_err(|e| anyhow::anyhow!("Base64 decode failed: {e}"))?;
                    Ok(decoded)
                } else {
                    bail!("No content in response for {}", gh.path);
                }
            }
            None => bail!("File not found: {}", gh.path),
        }
    }

    async fn write(&self, path: &str, _data: &[u8]) -> Result<()> {
        let _gh = Self::parse_path(path)?;

        // GitHub write requires authentication and is complex (create vs update)
        // For MVP, we make this read-only and defer write support
        bail!(
            "GitHub VFS is read-only. Use git clone for write access.\n\
             Path: {path}"
        )
    }

    async fn stat(&self, path: &str) -> Result<FileStat> {
        let gh = Self::parse_path(path)?;

        let content = self
            .client
            .repos(&gh.owner, &gh.repo)
            .get_content()
            .path(&gh.path)
            .r#ref(gh.branch.as_deref().unwrap_or("main"))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("GitHub API error: {e}"))?;

        match content.items.first() {
            Some(item) => {
                let is_dir = item.r#type == "dir";
                Ok(FileStat {
                    is_file: !is_dir,
                    is_dir,
                    size: item.size as u64,
                    created: None,
                    modified: None,
                    readonly: false, // GitHub allows writes with token
                })
            }
            None => bail!("Path not found: {}", gh.path),
        }
    }

    async fn list(&self, path: &str) -> Result<Vec<String>> {
        let gh = Self::parse_path(path)?;

        let contents = self
            .client
            .repos(&gh.owner, &gh.repo)
            .get_content()
            .path(&gh.path)
            .r#ref(gh.branch.as_deref().unwrap_or("main"))
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("GitHub API error: {e}"))?;

        let names: Vec<String> = contents.items.iter().map(|i| i.name.clone()).collect();
        Ok(names)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_simple_path() {
        let gh = GitHubFsBackend::parse_path("rust-lang/rust/README.md").unwrap();
        assert_eq!(gh.owner, "rust-lang");
        assert_eq!(gh.repo, "rust");
        assert_eq!(gh.path, "README.md");
        assert!(gh.branch.is_none());
    }

    #[test]
    fn parse_path_with_branch() {
        let gh = GitHubFsBackend::parse_path("owner/repo@dev/src/main.rs").unwrap();
        assert_eq!(gh.owner, "owner");
        assert_eq!(gh.repo, "repo");
        assert_eq!(gh.branch, Some("dev".to_string()));
        assert_eq!(gh.path, "src/main.rs");
    }

    #[test]
    fn parse_root_path() {
        let gh = GitHubFsBackend::parse_path("owner/repo").unwrap();
        assert_eq!(gh.owner, "owner");
        assert_eq!(gh.repo, "repo");
        assert_eq!(gh.path, "");
    }
}
