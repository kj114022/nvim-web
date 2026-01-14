//! SSH Integration Tests
//!
//! Requires ssh-test feature and a running SSH server.
//! Used by CI with Docker sshd container.
//!
//! Environment variables:
//! - NVIM_WEB_SSH_HOST: SSH host (default: localhost)
//! - NVIM_WEB_SSH_PORT: SSH port (default: 2222)
//! - NVIM_WEB_SSH_USER: Username (default: testuser)
//! - NVIM_WEB_SSH_PASS: Password (default: testpass)

#![cfg(feature = "ssh-test")]

use nvim_web_vfs::ssh::SshFsBackend;
use nvim_web_vfs::VfsBackend;
use std::env;

fn get_ssh_uri() -> String {
    let host = env::var("NVIM_WEB_SSH_HOST").unwrap_or_else(|_| "localhost".to_string());
    let port = env::var("NVIM_WEB_SSH_PORT").unwrap_or_else(|_| "2222".to_string());
    let user = env::var("NVIM_WEB_SSH_USER").unwrap_or_else(|_| "testuser".to_string());
    format!("vfs://ssh/{}@{}:{}/", user, host, port)
}

fn get_password() -> Option<String> {
    env::var("NVIM_WEB_SSH_PASS").ok()
}

/// Test SSH connection establishment
#[tokio::test]
async fn ssh_connect() {
    let uri = get_ssh_uri();
    let password = get_password();

    let result = SshFsBackend::connect_with_password(&uri, password.as_deref());
    assert!(result.is_ok(), "Failed to connect: {:?}", result.err());
}

/// Test reading a file via SSH
#[tokio::test]
async fn ssh_read_file() {
    let uri = get_ssh_uri();
    let password = get_password();

    let backend =
        SshFsBackend::connect_with_password(&uri, password.as_deref()).expect("Failed to connect");

    // Read /etc/passwd (should exist on any Linux)
    let result = backend.read("/etc/passwd").await;
    assert!(result.is_ok(), "Failed to read file: {:?}", result.err());

    let content = result.unwrap();
    assert!(!content.is_empty(), "File content should not be empty");
    assert!(
        String::from_utf8_lossy(&content).contains("root"),
        "/etc/passwd should contain 'root'"
    );
}

/// Test stat operation via SSH
#[tokio::test]
async fn ssh_stat_file() {
    let uri = get_ssh_uri();
    let password = get_password();

    let backend =
        SshFsBackend::connect_with_password(&uri, password.as_deref()).expect("Failed to connect");

    // Stat /etc/passwd
    let result = backend.stat("/etc/passwd").await;
    assert!(result.is_ok(), "Failed to stat file: {:?}", result.err());

    let stat = result.unwrap();
    assert!(stat.is_file, "/etc/passwd should be a file");
    assert!(!stat.is_dir, "/etc/passwd should not be a directory");
    assert!(stat.size > 0, "File size should be > 0");
}

/// Test listing directory via SSH
#[tokio::test]
async fn ssh_list_dir() {
    let uri = get_ssh_uri();
    let password = get_password();

    let backend =
        SshFsBackend::connect_with_password(&uri, password.as_deref()).expect("Failed to connect");

    // List /etc
    let result = backend.list("/etc").await;
    assert!(
        result.is_ok(),
        "Failed to list directory: {:?}",
        result.err()
    );

    let entries = result.unwrap();
    assert!(!entries.is_empty(), "Directory should have entries");
    assert!(
        entries.contains(&"passwd".to_string()),
        "/etc should contain 'passwd'"
    );
}

/// Test write and read cycle via SSH
#[tokio::test]
async fn ssh_write_read_cycle() {
    let uri = get_ssh_uri();
    let password = get_password();

    let backend =
        SshFsBackend::connect_with_password(&uri, password.as_deref()).expect("Failed to connect");

    let test_path = "/tmp/nvim-web-ssh-test.txt";
    let test_content = b"Hello from nvim-web SSH test!";

    // Write file
    let write_result = backend.write(test_path, test_content).await;
    assert!(
        write_result.is_ok(),
        "Failed to write file: {:?}",
        write_result.err()
    );

    // Read it back
    let read_result = backend.read(test_path).await;
    assert!(
        read_result.is_ok(),
        "Failed to read file: {:?}",
        read_result.err()
    );

    let content = read_result.unwrap();
    assert_eq!(content, test_content, "Content mismatch");
}

/// Test connection health check
#[tokio::test]
async fn ssh_connection_health() {
    let uri = get_ssh_uri();
    let password = get_password();

    let backend =
        SshFsBackend::connect_with_password(&uri, password.as_deref()).expect("Failed to connect");

    assert!(backend.is_alive(), "Connection should be alive");
}

/// Test stat on directory
#[tokio::test]
async fn ssh_stat_directory() {
    let uri = get_ssh_uri();
    let password = get_password();

    let backend =
        SshFsBackend::connect_with_password(&uri, password.as_deref()).expect("Failed to connect");

    let result = backend.stat("/tmp").await;
    assert!(
        result.is_ok(),
        "Failed to stat directory: {:?}",
        result.err()
    );

    let stat = result.unwrap();
    assert!(stat.is_dir, "/tmp should be a directory");
    assert!(!stat.is_file, "/tmp should not be a file");
}

/// Test error handling for non-existent file
#[tokio::test]
async fn ssh_read_nonexistent() {
    let uri = get_ssh_uri();
    let password = get_password();

    let backend =
        SshFsBackend::connect_with_password(&uri, password.as_deref()).expect("Failed to connect");

    let result = backend.read("/nonexistent/path/file.txt").await;
    assert!(result.is_err(), "Should fail for non-existent file");
}
