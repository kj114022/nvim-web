mod common;

use common::TestHarness;

/// Test that test harness can spawn Neovim and manage temp files
#[test]
fn test_harness_basic() {
    let harness = TestHarness::new().expect("Failed to create test harness");

    // Verify temp dir exists
    assert!(harness.tmp_dir.path().exists());

    // Verify we can write and read files
    harness.write_file("test.txt", "hello world").unwrap();
    let content = harness.read_file("test.txt").unwrap();
    assert_eq!(content, "hello world");
}

/// Test VFS backend integration - file read path
#[tokio::test]
async fn vfs_backend_reads_file_correctly() {
    use nvim_web_host::vfs::{LocalFs, VfsBackend};

    let harness = TestHarness::new().unwrap();
    harness.write_file("data.txt", "test data").unwrap();

    // Create LocalFs backend pointing to temp dir
    let backend = LocalFs::new(harness.tmp_dir.path());

    // Read via VFS backend
    let content = backend.read("data.txt").await.unwrap();
    assert_eq!(content, b"test data");
}

/// Test VFS backend integration - file write path
#[tokio::test]
async fn vfs_backend_writes_file_correctly() {
    use nvim_web_host::vfs::{LocalFs, VfsBackend};

    let harness = TestHarness::new().unwrap();
    let backend = LocalFs::new(harness.tmp_dir.path());

    // Write via VFS backend
    backend
        .write("output.txt", b"written by vfs")
        .await
        .unwrap();

    // Verify on disk
    let content = harness.read_file("output.txt").unwrap();
    assert_eq!(content, "written by vfs");
}

/// Test VFS manager path parsing
#[tokio::test]
async fn vfs_manager_parses_paths_correctly() {
    use nvim_web_host::vfs::{LocalFs, VfsManager};

    let harness = TestHarness::new().unwrap();
    let manager = VfsManager::new();
    manager
        .register_backend("local", Box::new(LocalFs::new(harness.tmp_dir.path())))
        .await;

    // Parse a VFS path
    let (backend, path) = manager
        .parse_vfs_path("vfs://local/test/file.txt")
        .await
        .unwrap();
    assert_eq!(backend, "local");
    assert_eq!(path, "test/file.txt");
}

/// Test VFS manager read integration
#[tokio::test]
async fn vfs_manager_reads_via_backend() {
    use nvim_web_host::vfs::{LocalFs, VfsManager};

    let harness = TestHarness::new().unwrap();
    harness.write_file("readme.txt", "VFS content").unwrap();

    let manager = VfsManager::new();
    manager
        .register_backend("local", Box::new(LocalFs::new(harness.tmp_dir.path())))
        .await;

    // Read via manager
    let content = manager.read_file("vfs://local/readme.txt").await.unwrap();
    assert_eq!(content, b"VFS content");
}

/// Test VFS manager write integration
#[tokio::test]
async fn vfs_manager_writes_via_backend() {
    use nvim_web_host::vfs::{LocalFs, VfsManager};

    let harness = TestHarness::new().unwrap();

    let manager = VfsManager::new();
    manager
        .register_backend("local", Box::new(LocalFs::new(harness.tmp_dir.path())))
        .await;

    // Write via manager
    manager
        .write_file("vfs://local/output.md", b"# VFS Test")
        .await
        .unwrap();

    // Verify on disk
    let content = harness.read_file("output.md").unwrap();
    assert_eq!(content, "# VFS Test");
}

/// Test buffer registration in VFS manager
#[tokio::test]
async fn vfs_manager_tracks_buffers() {
    use nvim_web_host::vfs::{LocalFs, VfsManager};

    let harness = TestHarness::new().unwrap();
    let manager = VfsManager::new();
    manager
        .register_backend("local", Box::new(LocalFs::new(harness.tmp_dir.path())))
        .await;

    // Register a buffer
    manager
        .register_buffer(42, "vfs://local/test.txt".to_string())
        .await
        .unwrap();

    // Retrieve it
    let managed = manager.get_managed_buffer(42).await.unwrap();
    assert_eq!(managed.vfs_path, "vfs://local/test.txt");
}
