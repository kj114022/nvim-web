//! Git VFS backend (read-only)
//!
//! Provides read-only access to files at specific Git commits.
//! Uses `git show` to retrieve file contents without full checkout.
//!
//! URI format: `vfs://git/repo@commit/path/to/file`
//! Examples:
//! - `vfs://git/.@HEAD/main.rs` - current repo, HEAD
//! - `vfs://git/.@abc123/src/lib.rs` - current repo, specific commit
//! - `vfs://git//path/to/repo@main/file.txt` - absolute repo path

use std::process::Command;

use anyhow::{bail, Context, Result};
use async_trait::async_trait;

use super::{FileStat, VfsBackend};

/// Git VFS backend for browsing repository history
pub struct GitFsBackend {
    /// Repository path (can be "." for current directory)
    repo_path: String,
}

impl GitFsBackend {
    /// Create a Git backend for the given repository path
    pub fn new(repo_path: impl Into<String>) -> Self {
        Self {
            repo_path: repo_path.into(),
        }
    }

    /// Parse a Git VFS path into (ref, file_path)
    /// Format: `ref/path/to/file` where ref can be HEAD, branch, tag, or commit hash
    fn parse_path(&self, path: &str) -> Result<(String, String)> {
        // Path format: "commit/path/to/file"
        // e.g., "HEAD/src/main.rs" or "abc123/lib.rs"
        let parts: Vec<&str> = path.splitn(2, '/').collect();
        
        if parts.len() < 2 {
            bail!("Invalid git path format. Expected: ref/path (e.g., HEAD/src/main.rs)");
        }

        Ok((parts[0].to_string(), parts[1].to_string()))
    }

    /// Run git show to get file contents at a specific ref
    fn git_show(&self, git_ref: &str, file_path: &str) -> Result<Vec<u8>> {
        let output = Command::new("git")
            .args(["-C", &self.repo_path, "show", &format!("{git_ref}:{file_path}")])
            .output()
            .context("Failed to run git show")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git show failed: {stderr}");
        }

        Ok(output.stdout)
    }

    /// List files at a specific ref and path
    fn git_ls_tree(&self, git_ref: &str, dir_path: &str) -> Result<Vec<String>> {
        let tree_path = if dir_path.is_empty() {
            git_ref.to_string()
        } else {
            format!("{git_ref}:{dir_path}")
        };

        let output = Command::new("git")
            .args(["-C", &self.repo_path, "ls-tree", "--name-only", &tree_path])
            .output()
            .context("Failed to run git ls-tree")?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            bail!("git ls-tree failed: {stderr}");
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let names: Vec<String> = stdout.lines().map(|s| s.to_string()).collect();
        
        Ok(names)
    }

    /// Check if path is a file or directory at given ref
    fn git_cat_file_type(&self, git_ref: &str, file_path: &str) -> Result<(bool, bool)> {
        let output = Command::new("git")
            .args([
                "-C", &self.repo_path,
                "cat-file", "-t",
                &format!("{git_ref}:{file_path}")
            ])
            .output()
            .context("Failed to run git cat-file")?;

        if !output.status.success() {
            bail!("Path not found in git: {git_ref}:{file_path}");
        }

        let obj_type = String::from_utf8_lossy(&output.stdout).trim().to_string();
        
        Ok((obj_type == "blob", obj_type == "tree"))
    }
}

impl Default for GitFsBackend {
    fn default() -> Self {
        Self::new(".")
    }
}

#[async_trait]
impl VfsBackend for GitFsBackend {
    async fn read(&self, path: &str) -> Result<Vec<u8>> {
        let (git_ref, file_path) = self.parse_path(path)?;
        self.git_show(&git_ref, &file_path)
    }

    async fn write(&self, _path: &str, _data: &[u8]) -> Result<()> {
        bail!("Git backend is read-only (use git commit to modify)")
    }

    async fn stat(&self, path: &str) -> Result<FileStat> {
        let (git_ref, file_path) = self.parse_path(path)?;
        let (is_file, is_dir) = self.git_cat_file_type(&git_ref, &file_path)?;

        let size = if is_file {
            // Get size via git cat-file -s
            Command::new("git")
                .args([
                    "-C", &self.repo_path,
                    "cat-file", "-s",
                    &format!("{git_ref}:{file_path}")
                ])
                .output()
                .ok()
                .and_then(|o| String::from_utf8_lossy(&o.stdout).trim().parse().ok())
                .unwrap_or(0)
        } else {
            0
        };

        Ok(FileStat {
            is_file,
            is_dir,
            size,
            created: None,
            modified: None,
            readonly: true, // Git backend is read-only
        })
    }

    async fn list(&self, path: &str) -> Result<Vec<String>> {
        let (git_ref, dir_path) = self.parse_path(path)?;
        self.git_ls_tree(&git_ref, &dir_path)
    }
}
