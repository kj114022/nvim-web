use nvim_web_host::vfs::{VfsBackend, LocalFs, BrowserFsBackend};
use std::sync::mpsc;
use tempfile::TempDir;

mod common;
use common::mock_browser::MockBrowserFs;

/// Backend selector for parameterized tests
#[derive(Clone, Copy, Debug)]
pub enum BackendKind {
    Local,
    Browser,
}

/// Test fixture for VFS backend testing
/// 
/// Provides a VfsBackend implementation based on BackendKind.
/// For LocalFs: uses temp directory
/// For BrowserFs: uses mock browser WS peer with in-memory storage
struct BackendFixture {
    backend: Box<dyn VfsBackend>,
    _temp_dir: Option<TempDir>,  // Keep alive for LocalFs
    _mock_thread: Option<std::thread::JoinHandle<()>>,  // Keep alive for BrowserFs
}

impl BackendFixture {
    fn new(kind: BackendKind) -> Self {
        match kind {
            BackendKind::Local => {
                let temp_dir = TempDir::new().unwrap();
                let backend = Box::new(LocalFs::new(temp_dir.path()));
                BackendFixture {
                    backend,
                    _temp_dir: Some(temp_dir),
                    _mock_thread: None,
                }
            }
            BackendKind::Browser => {
                // Create channels for mock browser communication
                let (to_browser_tx, to_browser_rx) = mpsc::channel();
                let (from_browser_tx, from_browser_rx) = mpsc::channel();
                
                // Spawn mock browser FS service
                let mock_thread = MockBrowserFs::spawn(to_browser_rx, from_browser_tx);
                
                // Create BrowserFsBackend with channel to mock
                let backend = Box::new(BrowserFsBackend::new("test", to_browser_tx));
                
                // Spawn response handler thread
                std::thread::spawn(move || {
                    for response in from_browser_rx {
                        let mut cursor = std::io::Cursor::new(response);
                        if let Ok(value) = rmpv::decode::read_value(&mut cursor) {
                            let _ = nvim_web_host::rpc_sync::handle_fs_response(&value);
                        }
                    }
                });
                
                BackendFixture {
                    backend,
                    _temp_dir: None,
                    _mock_thread: Some(mock_thread),
                }
            }
        }
    }
    
    fn backend(&self) -> &dyn VfsBackend {
        &*self.backend
    }
}

// Test A: Read/Write roundtrip
fn test_read_write_roundtrip(kind: BackendKind) {
    let fixture = BackendFixture::new(kind);
    let backend = fixture.backend();
    
    let path = "test.txt";
    let content = b"Hello, VFS!";
    
    // Write
    backend.write(path, content).unwrap();
    
    // Read back
    let read_content = backend.read(path).unwrap();
    assert_eq!(read_content, content);
}

#[test]
fn read_write_roundtrip_local() {
    test_read_write_roundtrip(BackendKind::Local);
}

#[test]
fn read_write_roundtrip_browser() {
    test_read_write_roundtrip(BackendKind::Browser);
}

// Test B: Overwrite existing file
fn test_overwrite(kind: BackendKind) {
    let fixture = BackendFixture::new(kind);
    let backend = fixture.backend();
    
    let path = "overwrite.txt";
    
    // Initial write
    backend.write(path, b"original").unwrap();
    
    // Overwrite
    backend.write(path, b"modified").unwrap();
    
    // Verify
    let content = backend.read(path).unwrap();
    assert_eq!(content, b"modified");
}

#[test]
fn overwrite_local() {
    test_overwrite(BackendKind::Local);
}

#[test]
fn overwrite_browser() {
    test_overwrite(BackendKind::Browser);
}

// Test C: Read non-existent file
fn test_read_nonexistent(kind: BackendKind) {
    let fixture = BackendFixture::new(kind);
    let backend = fixture.backend();
    
    let result = backend.read("nonexistent.txt");
    assert!(result.is_err());
}

#[test]
fn read_nonexistent_local() {
    test_read_nonexistent(BackendKind::Local);
}

#[test]
fn read_nonexistent_browser() {
    test_read_nonexistent(BackendKind::Browser);
}

// Test D: Binary data handling
fn test_binary_data(kind: BackendKind) {
    let fixture = BackendFixture::new(kind);
    let backend = fixture.backend();
    
    let path = "binary.dat";
    let binary_data: Vec<u8> = (0..=255).collect();
    
    backend.write(path, &binary_data).unwrap();
    let read_data = backend.read(path).unwrap();
    
    assert_eq!(read_data, binary_data);
}

#[test]
fn binary_data_local() {
    test_binary_data(BackendKind::Local);
}

#[test]
fn binary_data_browser() {
    test_binary_data(BackendKind::Browser);
}

// Test E: Empty file
fn test_empty_file(kind: BackendKind) {
    let fixture = BackendFixture::new(kind);
    let backend = fixture.backend();
    
    let path = "empty.txt";
    backend.write(path, b"").unwrap();
    
    let content = backend.read(path).unwrap();
    assert_eq!(content, b"");
}

#[test]
fn empty_file_local() {
    test_empty_file(BackendKind::Local);
}

#[test]
fn empty_file_browser() {
    test_empty_file(BackendKind::Browser);
}
