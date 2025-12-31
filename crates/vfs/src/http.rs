//! HTTP/Fetch VFS backend (read-only)
//!
//! Provides read-only access to remote files via HTTP/HTTPS.
//! Useful for viewing remote configuration files, documentation, etc.
//!
//! URI format: `vfs://http/https://example.com/file.txt`
//! or: `vfs://http/http://example.com/file.txt`

use anyhow::{bail, Result};
use async_trait::async_trait;

use super::{FileStat, VfsBackend};

/// HTTP VFS backend for read-only remote file access
pub struct HttpFsBackend {
    /// HTTP client (uses reqwest)
    client: reqwest::Client,
    /// Base URL (optional, for relative paths)
    base_url: Option<String>,
}

impl HttpFsBackend {
    /// Create a new HTTP backend
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            base_url: None,
        }
    }

    /// Create with a base URL for relative paths
    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            base_url: Some(base_url.into()),
        }
    }

    /// Resolve path to full URL
    fn resolve_url(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            path.to_string()
        } else if let Some(base) = &self.base_url {
            format!("{}/{}", base.trim_end_matches('/'), path.trim_start_matches('/'))
        } else {
            format!("https://{path}")
        }
    }
}

impl Default for HttpFsBackend {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl VfsBackend for HttpFsBackend {
    async fn read(&self, path: &str) -> Result<Vec<u8>> {
        let url = self.resolve_url(path);
        
        let response = self.client
            .get(&url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("HTTP request failed: {e}"))?;

        if !response.status().is_success() {
            bail!("HTTP {} for {}", response.status(), url);
        }

        let bytes = response
            .bytes()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to read response: {e}"))?;

        Ok(bytes.to_vec())
    }

    async fn write(&self, _path: &str, _data: &[u8]) -> Result<()> {
        bail!("HTTP backend is read-only")
    }

    async fn stat(&self, path: &str) -> Result<FileStat> {
        let url = self.resolve_url(path);
        
        // HEAD request to get metadata
        let response = self.client
            .head(&url)
            .send()
            .await
            .map_err(|e| anyhow::anyhow!("HTTP HEAD failed: {e}"))?;

        if !response.status().is_success() {
            bail!("HTTP {} for {}", response.status(), url);
        }

        let size = response
            .headers()
            .get("content-length")
            .and_then(|v| v.to_str().ok())
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        Ok(FileStat {
            is_file: true,
            is_dir: false,
            size,
            created: None,
            modified: None,
            readonly: true, // HTTP is read-only
        })
    }

    async fn list(&self, _path: &str) -> Result<Vec<String>> {
        bail!("HTTP backend does not support directory listing")
    }
}
