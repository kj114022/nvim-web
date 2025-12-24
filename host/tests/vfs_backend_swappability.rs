//! VFS Backend Swappability Tests
//!
//! Tests that LocalFs backend works correctly.
//! Browser tests are temporarily disabled pending async migration.

use nvim_web_host::vfs::{VfsBackend, LocalFs};
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
#[test]
fn read_write_roundtrip_local() {
    let fixture = BackendFixture::new_local();
    let backend = fixture.backend();
    
    let path = "test.txt";
    let content = b"Hello, VFS!";
    
    backend.write(path, content).unwrap();
    let read_content = backend.read(path).unwrap();
    assert_eq!(read_content, content);
}

// Test B: Overwrite existing file
#[test]
fn overwrite_local() {
    let fixture = BackendFixture::new_local();
    let backend = fixture.backend();
    
    let path = "overwrite.txt";
    
    backend.write(path, b"original").unwrap();
    backend.write(path, b"modified").unwrap();
    
    let content = backend.read(path).unwrap();
    assert_eq!(content, b"modified");
}

// Test C: Read non-existent file
#[test]
fn read_nonexistent_local() {
    let fixture = BackendFixture::new_local();
    let backend = fixture.backend();
    
    let result = backend.read("nonexistent.txt");
    assert!(result.is_err());
}

// Test D: Binary data handling
#[test]
fn binary_data_local() {
    let fixture = BackendFixture::new_local();
    let backend = fixture.backend();
    
    let path = "binary.dat";
    let binary_data: Vec<u8> = (0..=255).collect();
    
    backend.write(path, &binary_data).unwrap();
    let read_data = backend.read(path).unwrap();
    
    assert_eq!(read_data, binary_data);
}

// Test E: Empty file
#[test]
fn empty_file_local() {
    let fixture = BackendFixture::new_local();
    let backend = fixture.backend();
    
    let path = "empty.txt";
    backend.write(path, b"").unwrap();
    
    let content = backend.read(path).unwrap();
    assert_eq!(content, b"");
}

// Browser tests temporarily disabled - pending async migration
// TODO: Re-enable once BrowserFsBackend is migrated to async
