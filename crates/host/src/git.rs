//! Git repository detection and branch retrieval
//!
//! Provides utilities for finding git root directories and current branch names.

use std::path::{Path, PathBuf};

/// Find the git root directory by walking up from the given path
///
/// Returns the path containing the `.git` directory, or None if not in a git repo.
pub fn find_git_root(path: &Path) -> Option<PathBuf> {
    let mut current = if path.is_file() { path.parent()? } else { path };

    loop {
        let git_dir = current.join(".git");
        if git_dir.exists() {
            return Some(current.to_path_buf());
        }

        match current.parent() {
            Some(parent) => current = parent,
            None => return None,
        }
    }
}

/// Get the current git branch name from HEAD
///
/// Reads `.git/HEAD` and parses the ref to extract branch name.
/// Returns None if not on a branch (detached HEAD) or on error.
pub fn get_current_branch(git_root: &Path) -> Option<String> {
    let head_path = git_root.join(".git/HEAD");
    let content = std::fs::read_to_string(head_path).ok()?;

    // HEAD format: "ref: refs/heads/<branch>\n" or "<sha>" for detached
    let content = content.trim();

    if content.starts_with("ref: refs/heads/") {
        Some(content.trim_start_matches("ref: refs/heads/").to_string())
    } else {
        // Detached HEAD - return short SHA
        if content.len() >= 7 {
            Some(content[..7].to_string())
        } else {
            None
        }
    }
}

/// Find git root using git command (more reliable, handles worktrees)
///
/// Falls back to filesystem traversal if git command fails.
#[allow(dead_code)]
pub fn find_git_root_via_command(path: &Path) -> Option<PathBuf> {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .current_dir(path)
        .output()
        .ok()?;

    if output.status.success() {
        let path_str = String::from_utf8_lossy(&output.stdout);
        Some(PathBuf::from(path_str.trim()))
    } else {
        // Fallback to filesystem traversal
        find_git_root(path)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    #[test]
    fn test_find_git_root() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path();

        // Create .git directory
        fs::create_dir(repo.join(".git")).unwrap();

        // Create nested directory
        let nested = repo.join("src").join("nested");
        fs::create_dir_all(&nested).unwrap();

        // Test detection from nested directory
        let git_root = find_git_root(&nested);
        assert_eq!(git_root, Some(repo.to_path_buf()));
    }

    #[test]
    fn test_no_git_repo() {
        let temp = TempDir::new().unwrap();
        let git_root = find_git_root(temp.path());
        assert_eq!(git_root, None);
    }

    #[test]
    fn test_get_branch_name() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path();

        // Create .git directory with HEAD
        fs::create_dir(repo.join(".git")).unwrap();
        fs::write(repo.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();

        let branch = get_current_branch(repo);
        assert_eq!(branch, Some("main".to_string()));
    }

    #[test]
    fn test_detached_head() {
        let temp = TempDir::new().unwrap();
        let repo = temp.path();

        // Create .git directory with detached HEAD
        fs::create_dir(repo.join(".git")).unwrap();
        fs::write(repo.join(".git/HEAD"), "abc123def456789").unwrap();

        let branch = get_current_branch(repo);
        assert_eq!(branch, Some("abc123d".to_string()));
    }
}
