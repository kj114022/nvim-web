//! VFS Backend Swappability Tests
//!
//! Tests that LocalFs backend works correctly.
//! Browser tests are temporarily disabled pending WebSocket integration.

use nvim_web_host::vfs::{LocalFs, VfsBackend};
use tempfile::TempDir;

mod common;

/// Test fixture for VFS backend testing
struct BackendFixture {
    backend: Box<dyn VfsBackend>,
    _temp_dir: TempDir,
}

impl BackendFixture {
    fn new_local() -> Self {
        let temp_dir = TempDir::new().unwrap();
        let backend = Box::new(LocalFs::new(temp_dir.path()));
        BackendFixture {
            backend,
            _temp_dir: temp_dir,
        }
    }

    fn backend(&self) -> &dyn VfsBackend {
        &*self.backend
    }
}

// Test A: Read/Write roundtrip
#[tokio::test]
async fn read_write_roundtrip_local() {
    let fixture = BackendFixture::new_local();
    let backend = fixture.backend();

    let path = "test.txt";
    let content = b"Hello, VFS!";

    backend.write(path, content).await.unwrap();
    let read_content = backend.read(path).await.unwrap();
    assert_eq!(read_content, content);
}

// Test B: Overwrite existing file
#[tokio::test]
async fn overwrite_local() {
    let fixture = BackendFixture::new_local();
    let backend = fixture.backend();

    let path = "overwrite.txt";

    backend.write(path, b"original").await.unwrap();
    backend.write(path, b"modified").await.unwrap();

    let content = backend.read(path).await.unwrap();
    assert_eq!(content, b"modified");
}

// Test C: Read non-existent file
#[tokio::test]
async fn read_nonexistent_local() {
    let fixture = BackendFixture::new_local();
    let backend = fixture.backend();

    let result = backend.read("nonexistent.txt").await;
    assert!(result.is_err());
}

// Test D: Binary data handling
#[tokio::test]
async fn binary_data_local() {
    let fixture = BackendFixture::new_local();
    let backend = fixture.backend();

    let path = "binary.dat";
    let binary_data: Vec<u8> = (0..=255).collect();

    backend.write(path, &binary_data).await.unwrap();
    let read_data = backend.read(path).await.unwrap();

    assert_eq!(read_data, binary_data);
}

// Test E: Empty file
#[tokio::test]
async fn empty_file_local() {
    let fixture = BackendFixture::new_local();
    let backend = fixture.backend();

    let path = "empty.txt";
    backend.write(path, b"").await.unwrap();

    let content = backend.read(path).await.unwrap();
    assert_eq!(content, b"");
}

// Browser tests temporarily disabled - pending WebSocket integration
// TODO: Re-enable once BrowserFsBackend is wired to ws.rs
