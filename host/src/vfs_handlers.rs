use std::io::Write;
use anyhow::{Result, Context};
use rmpv::Value;

use crate::vfs::VfsManager;

/// Handle open_vfs RPC command
pub fn handle_open_vfs(
    vfs_manager: &mut VfsManager,
    nvim_stdin: &mut impl Write,
    vfs_path: String,
) -> Result<()> {
    eprintln!("Opening VFS file: {}", vfs_path);
    
    // 1. Validate and parse VFS path
    let (_backend_name, _file_path) = vfs_manager.parse_vfs_path(&vfs_path)
        .context("Failed to parse VFS path")?;
    
    // 2. Read file via backend
    let bytes = vfs_manager.read_file(&vfs_path)
        .context("Failed to read VFS file")?;
    
    // 3. Convert bytes to lines
    let content = String::from_utf8_lossy(&bytes);
    let lines: Vec<String> = content.lines().map(|s| s.to_string()).collect();
    
    // 4. Create buffer and get real buffer ID via RPC
    let bufnr_value = crate::rpc_sync::rpc_call(
        nvim_stdin,
        "nvim_create_buf",
        vec![Value::Boolean(true), Value::Boolean(false)],
    )?;
    
    let bufnr = bufnr_value.as_i64()
        .context("Buffer ID not an integer")?  as u32;
    
    eprintln!("Created buffer with ID: {}", bufnr);
    
    // 5. Set buffer name
    crate::rpc_sync::rpc_call(
        nvim_stdin,
        "nvim_buf_set_name",
        vec![Value::Integer(bufnr.into()), Value::String(vfs_path.clone().into())],
    )?;
    
    // 6. Set options BEFORE setting lines (critical order)
    crate::rpc_sync::rpc_call(
        nvim_stdin,
        "nvim_buf_set_option",
        vec![Value::Integer(bufnr.into()), Value::String("buftype".into()), Value::String("acwrite".into())],
    )?;
    
    crate::rpc_sync::rpc_call(
        nvim_stdin,
        "nvim_buf_set_option",
        vec![Value::Integer(bufnr.into()), Value::String("swapfile".into()), Value::Boolean(false)],
    )?;
    
    crate::rpc_sync::rpc_call(
        nvim_stdin,
        "nvim_buf_set_option",
        vec![Value::Integer(bufnr.into()), Value::String("undofile".into()), Value::Boolean(false)],
    )?;
    
    // 7. Set buffer contents
    let line_values: Vec<Value> = lines.into_iter().map(|s| Value::String(s.into())).collect();
    crate::rpc_sync::rpc_call(
        nvim_stdin,
        "nvim_buf_set_lines",
        vec![
            Value::Integer(bufnr.into()),
            Value::Integer(0.into()),
            Value::Integer((-1).into()),
            Value::Boolean(false),
            Value::Array(line_values),
        ],
    )?;
    
    // 8. Make current buffer
    crate::rpc_sync::rpc_call(
        nvim_stdin,
        "nvim_set_current_buf",
        vec![Value::Integer(bufnr.into())],
    )?;
    
    // 9. Register in VFS manager
    vfs_manager.register_buffer(bufnr, vfs_path.clone())?;
    
    eprintln!("VFS file opened successfully: {} (buffer {})", vfs_path, bufnr);
    Ok(())
}

/// Handle write_vfs RPC command
pub fn handle_write_vfs(
    vfs_manager: &mut VfsManager,
    nvim_stdin: &mut impl Write,
    bufnr: u32,
) -> Result<()> {
    eprintln!("Writing VFS buffer: {}", bufnr);
    
    // 1. Lookup buffer in registry
    let managed = vfs_manager.get_managed_buffer(bufnr)
        .context("Buffer not managed by VFS")?;
    
    let vfs_path = managed.vfs_path.clone();
    
    // 2. Get buffer lines via RPC
    let lines_value = crate::rpc_sync::rpc_call(
        nvim_stdin,
        "nvim_buf_get_lines",
        vec![
            Value::Integer(bufnr.into()),
            Value::Integer(0.into()),
            Value::Integer((-1).into()),
            Value::Boolean(false),
        ],
    )?;
    
    // 3. Extract lines from response
    let lines = if let Value::Array(arr) = lines_value {
        arr.iter()
            .filter_map(|v| {
                if let Value::String(s) = v {
                    s.as_str().map(|s| s.to_string())
                } else {
                    None
                }
            })
            .collect::<Vec<String>>()
    } else {
        anyhow::bail!("Expected array of lines from nvim_buf_get_lines");
    };
    
    // 4. Join lines and convert to bytes
    let content = lines.join("\n");
    let bytes = content.as_bytes();
    
    // 5. Write via backend
    vfs_manager.write_file(&vfs_path, bytes)
        .context("Failed to write VFS file")?;
    
    // 6. Clear modified flag
    crate::rpc_sync::rpc_call(
        nvim_stdin,
        "nvim_buf_set_option",
        vec![
            Value::Integer(bufnr.into()),
            Value::String("modified".into()),
            Value::Boolean(false),
        ],
    )?;
    
    eprintln!("VFS buffer {} written successfully to {}", bufnr, vfs_path);
    Ok(())
}
