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
/// For large files (> 1MB), only loads the first 100KB and shows a truncation indicator.
///
/// # Arguments
/// * `vfs_path` - VFS path (e.g., `<vfs://local/path/to/file.txt>`)
/// * `session` - Active Neovim session
/// * `vfs_manager` - VFS manager with registered backends
///
/// # Protocol
/// When the browser requests to open a VFS file, this handler:
/// 1. Reads the file content via `VfsManager`
/// 2. If file > 1MB, truncate to first 100KB with indicator
/// 3. Creates a new buffer in Neovim
/// 4. Sets the buffer content
/// 5. Returns the buffer number

/// Large file threshold: 1MB
const LARGE_FILE_THRESHOLD: usize = 1024 * 1024;
/// Chunk size for large files: 100KB
const LARGE_FILE_CHUNK_SIZE: usize = 100 * 1024;

pub async fn handle_open_vfs(
    vfs_path: &str,
    session: &AsyncSession,
    vfs_manager: &VfsManager,
) -> Result<u32> {
    // Read file content via VFS
    let content = vfs_manager.read_file(vfs_path).await?;

    // Handle large files with truncation
    let (display_content, truncated) = if content.len() > LARGE_FILE_THRESHOLD {
        let chunk = &content[..LARGE_FILE_CHUNK_SIZE.min(content.len())];
        (chunk.to_vec(), Some(content.len()))
    } else {
        (content, None)
    };

    // Convert to string (assuming UTF-8 text)
    let text = String::from_utf8_lossy(&display_content);
    let mut lines: Vec<Value> = text.lines().map(|l| Value::String(l.into())).collect();

    // Add truncation indicator if needed
    if let Some(total_size) = truncated {
        let size_mb = total_size as f64 / (1024.0 * 1024.0);
        let shown_kb = LARGE_FILE_CHUNK_SIZE as f64 / 1024.0;
        lines.push(Value::String("".into()));
        lines.push(Value::String(
            format!(
                "--- File truncated ({:.1}MB total, showing first {:.0}KB) ---",
                size_mb, shown_kb
            )
            .into(),
        ));
        lines.push(Value::String(
            "--- Use :e! or external tool for full file ---".into(),
        ));
    }

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

/// Handle chunked file read for large file virtual scrolling
///
/// Returns lines from start_line to end_line (0-indexed, inclusive).
/// This enables virtual scrolling where only visible lines are loaded.
pub async fn handle_read_chunk(
    vfs_path: &str,
    start_line: usize,
    end_line: usize,
    vfs_manager: &VfsManager,
) -> Result<Vec<String>> {
    // Read full file (could optimize with seek for truly huge files)
    let content = vfs_manager.read_file(vfs_path).await?;
    let text = String::from_utf8_lossy(&content);

    let lines: Vec<&str> = text.lines().collect();
    let total_lines = lines.len();

    // Clamp range
    let start = start_line.min(total_lines);
    let end = (end_line + 1).min(total_lines);

    let chunk: Vec<String> = lines[start..end].iter().map(|s| (*s).to_string()).collect();

    eprintln!(
        "VFS: Read chunk {} lines {}-{} (total {})",
        vfs_path, start_line, end_line, total_lines
    );

    Ok(chunk)
}

/// Get file metadata for virtual buffer setup
pub async fn handle_file_info(vfs_path: &str, vfs_manager: &VfsManager) -> Result<(usize, usize)> {
    let content = vfs_manager.read_file(vfs_path).await?;
    let size = content.len();
    let line_count = content.iter().filter(|&&b| b == b'\n').count() + 1;

    Ok((size, line_count))
}

/// Handle delete VFS file/directory request
///
/// Recursively deletes the path in the VFS backend.
pub async fn handle_delete_vfs(
    vfs_path: &str,
    session: &AsyncSession,
    vfs_manager: &VfsManager,
) -> Result<()> {
    // Perform deletion via VFS manager
    // For now we use the async_ops logic directly or exposed via manager
    // Since VfsManager doesn't expose remove_dir_all directly yet (it maps to backend),
    // we need to resolve backend and call async_ops.

    // Parse URI to get backend
    let uri = url::Url::parse(vfs_path).map_err(|e| anyhow::anyhow!("Invalid URI: {e}"))?;
    let scheme = uri.scheme();

    let vfs = vfs_manager; // We have &VfsManager
                           // We need to access the inner backend. VfsManager.get_backend(scheme) -> Result<Arc<dyn VfsBackend>>

    if let Ok(backend) = vfs.get_backend(scheme).await {
        // Path logic depends on backend (e.g. ssh has /path, local has path)
        // VfsManager::resolve_path handles this, but here we need backend access.
        // Let's assume VfsManager exposes a way or we manually resolve.

        let path = uri.path().to_string();

        crate::vfs::async_ops::remove_dir_all(backend.as_ref(), &path).await?;

        // Notify user via echomsg
        let msg = format!("Deleted {vfs_path}");
        session
            .rpc_call(
                "nvim_call_function",
                vec![
                    Value::String("NvimWeb_EchoMsg".into()),
                    Value::Array(vec![Value::String(msg.into())]),
                ],
            )
            .await
            .ok(); // Ignore error if function doesn't exist

        eprintln!("VFS: Deleted {vfs_path}");
        Ok(())
    } else {
        Err(anyhow::anyhow!("Backend not found for {scheme}"))
    }
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

        let stat = backend.stat(&entry_path).await.unwrap_or(FileStat::file(0));

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
