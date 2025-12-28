//! VFS handlers for opening and writing virtual files
//!
//! These handlers integrate the VFS backend with Neovim buffers,
//! enabling seamless file operations across different storage backends
//! (`LocalFs`, SSH, `BrowserFs`).
//!
//! The handlers use the async `VfsManager` to read/write files and
//! the nvim-rs API to manipulate Neovim buffers.

use anyhow::Result;
use rmpv::Value;

use crate::session::AsyncSession;
use crate::vfs::{FileStat, VfsBackend, VfsManager};

/// File tree entry for explorer
#[derive(Debug, Clone)]
pub struct TreeEntry {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size: u64,
    pub children: Option<Vec<Self>>,
}

impl TreeEntry {
    /// Convert to `MessagePack` Value for RPC
    pub fn to_value(&self) -> Value {
        let mut map = vec![
            (
                Value::String("name".into()),
                Value::String(self.name.clone().into()),
            ),
            (
                Value::String("path".into()),
                Value::String(self.path.clone().into()),
            ),
            (Value::String("is_dir".into()), Value::Boolean(self.is_dir)),
            (
                Value::String("size".into()),
                Value::Integer(self.size.into()),
            ),
        ];

        if let Some(ref children) = self.children {
            let children_val: Vec<Value> = children.iter().map(Self::to_value).collect();
            map.push((Value::String("children".into()), Value::Array(children_val)));
        }

        Value::Map(map)
    }
}

/// Handle open VFS file request
///
/// Reads file content from VFS backend and loads it into a new Neovim buffer.
///
/// # Arguments
/// * `vfs_path` - VFS path (e.g., `<vfs://local/path/to/file.txt>`)
/// * `session` - Active Neovim session
/// * `vfs_manager` - VFS manager with registered backends
///
/// # Protocol
/// When the browser requests to open a VFS file, this handler:
/// 1. Reads the file content via `VfsManager`
/// 2. Creates a new buffer in Neovim
/// 3. Sets the buffer content
/// 4. Returns the buffer number
pub async fn handle_open_vfs(
    vfs_path: &str,
    session: &AsyncSession,
    vfs_manager: &VfsManager,
) -> Result<u32> {
    // Read file content via VFS
    let content = vfs_manager.read_file(vfs_path).await?;

    // Convert to string (assuming UTF-8 text)
    let text = String::from_utf8_lossy(&content);
    let lines: Vec<Value> = text.lines().map(|l| Value::String(l.into())).collect();

    // Create new buffer
    let bufnr_result = session
        .rpc_call(
            "nvim_create_buf",
            vec![
                Value::Boolean(true),  // listed
                Value::Boolean(false), // scratch
            ],
        )
        .await?;

    let bufnr = u32::try_from(
        bufnr_result
            .as_u64()
            .ok_or_else(|| anyhow::anyhow!("Failed to create buffer"))?,
    )?;

    // Set buffer name to VFS path
    session
        .rpc_call(
            "nvim_buf_set_name",
            vec![Value::Integer(bufnr.into()), Value::String(vfs_path.into())],
        )
        .await?;

    // Set buffer content
    session
        .rpc_call(
            "nvim_buf_set_lines",
            vec![
                Value::Integer(bufnr.into()),
                Value::Integer(0.into()),    // start
                Value::Integer((-1).into()), // end (all lines)
                Value::Boolean(false),       // strict_indexing
                Value::Array(lines),
            ],
        )
        .await?;

    // Mark buffer as not modified (since we just loaded it)
    session
        .rpc_call(
            "nvim_buf_set_option",
            vec![
                Value::Integer(bufnr.into()),
                Value::String("modified".into()),
                Value::Boolean(false),
            ],
        )
        .await?;

    // Set buffer type for VFS files
    session
        .rpc_call(
            "nvim_buf_set_option",
            vec![
                Value::Integer(bufnr.into()),
                Value::String("buftype".into()),
                Value::String("acwrite".into()), // auto-command write
            ],
        )
        .await?;

    eprintln!("VFS: Opened {vfs_path} in buffer {bufnr}");

    Ok(bufnr)
}

/// Handle write VFS file request
///
/// Gets content from Neovim buffer and writes it to VFS backend.
///
/// # Arguments
/// * `vfs_path` - VFS path to write to
/// * `bufnr` - Neovim buffer number
/// * `session` - Active Neovim session
/// * `vfs_manager` - VFS manager with registered backends
pub async fn handle_write_vfs(
    vfs_path: &str,
    bufnr: u32,
    session: &AsyncSession,
    vfs_manager: &VfsManager,
) -> Result<()> {
    // Get buffer lines
    let lines_result = session
        .rpc_call(
            "nvim_buf_get_lines",
            vec![
                Value::Integer(bufnr.into()),
                Value::Integer(0.into()),    // start
                Value::Integer((-1).into()), // end (all lines)
                Value::Boolean(false),       // strict_indexing
            ],
        )
        .await?;

    // Convert lines to string
    let content = if let Value::Array(lines) = lines_result {
        let text_lines: Vec<String> = lines
            .into_iter()
            .filter_map(|v| v.as_str().map(ToString::to_string))
            .collect();
        text_lines.join("\n")
    } else {
        return Err(anyhow::anyhow!("Failed to get buffer content"));
    };

    // Write to VFS
    vfs_manager.write_file(vfs_path, content.as_bytes()).await?;

    // Mark buffer as not modified
    session
        .rpc_call(
            "nvim_buf_set_option",
            vec![
                Value::Integer(bufnr.into()),
                Value::String("modified".into()),
                Value::Boolean(false),
            ],
        )
        .await?;

    eprintln!("VFS: Wrote buffer {bufnr} to {vfs_path}");

    Ok(())
}

/// List directory tree for file explorer
///
/// Returns a tree structure of files and directories.
///
/// # Arguments
/// * `path` - Directory path to list
/// * `depth` - Maximum recursion depth (0 = no recursion)
/// * `backend` - VFS backend to use
pub async fn handle_list_tree(
    path: &str,
    depth: usize,
    backend: &dyn VfsBackend,
) -> Result<Vec<TreeEntry>> {
    let entries = backend.list(path).await?;
    let mut tree = Vec::new();

    for name in entries {
        let entry_path = if path == "/" || path.is_empty() {
            format!("/{name}")
        } else {
            format!("{}/{}", path.trim_end_matches('/'), name)
        };

        let stat = backend.stat(&entry_path).await.unwrap_or(FileStat {
            is_file: true,
            is_dir: false,
            size: 0,
        });

        let children = if stat.is_dir && depth > 0 {
            // Box::pin required for async recursion
            Box::pin(handle_list_tree(&entry_path, depth - 1, backend))
                .await
                .ok()
        } else {
            None
        };

        tree.push(TreeEntry {
            name,
            path: entry_path,
            is_dir: stat.is_dir,
            size: stat.size,
            children,
        });
    }

    // Sort: directories first, then alphabetically
    tree.sort_by(|a, b| match (a.is_dir, b.is_dir) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(tree)
}

/// Convert tree entries to Value for RPC response
pub fn tree_to_value(tree: &[TreeEntry]) -> Value {
    Value::Array(tree.iter().map(TreeEntry::to_value).collect())
}

/// VFS status - returns current implementation status
pub const fn vfs_status() -> &'static str {
    "VFS handlers are fully async and operational"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vfs_status_reports_operational() {
        assert!(vfs_status().contains("operational"));
    }

    #[test]
    fn tree_entry_to_value() {
        let entry = TreeEntry {
            name: "test.txt".to_string(),
            path: "/test.txt".to_string(),
            is_dir: false,
            size: 100,
            children: None,
        };

        let val = entry.to_value();
        assert!(matches!(val, Value::Map(_)));
    }
}
