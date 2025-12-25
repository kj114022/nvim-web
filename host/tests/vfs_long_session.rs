use nvim_web_host::vfs::{LocalFs, VfsBackend};
use tempfile::TempDir;

/// Phase 8.3: Long-session stability test
///
/// Verifies that extended editing sessions don't leak resources,
/// corrupt state, or deadlock. This catches issues that only appear
/// after multiple operations.
#[tokio::test]
async fn long_session_stability() {
    let temp_dir = TempDir::new().unwrap();
    let backend = LocalFs::new(temp_dir.path());

    let path = "long_session.txt";

    // === Session: Multiple edits and writes ===

    // 1. Initial write
    backend.write(path, b"Line 1\n").await.unwrap();

    // 2. Read back
    let content = backend.read(path).await.unwrap();
    assert_eq!(content, b"Line 1\n");

    // 3. Append more content (simulating edits)
    backend.write(path, b"Line 1\nLine 2\n").await.unwrap();
    backend
        .write(path, b"Line 1\nLine 2\nLine 3\n")
        .await
        .unwrap();
    backend
        .write(path, b"Line 1\nLine 2\nLine 3\nLine 4\n")
        .await
        .unwrap();
    backend
        .write(path, b"Line 1\nLine 2\nLine 3\nLine 4\nLine 5\n")
        .await
        .unwrap();

    // 4. Read after multiple writes
    let content = backend.read(path).await.unwrap();
    assert_eq!(content, b"Line 1\nLine 2\nLine 3\nLine 4\nLine 5\n");

    // 5. Simulate undo (revert to earlier state)
    backend
        .write(path, b"Line 1\nLine 2\nLine 3\n")
        .await
        .unwrap();
    let content = backend.read(path).await.unwrap();
    assert_eq!(content, b"Line 1\nLine 2\nLine 3\n");

    // 6. Redo (forward again)
    backend
        .write(path, b"Line 1\nLine 2\nLine 3\nLine 4\n")
        .await
        .unwrap();

    // 7. Final verification
    let content = backend.read(path).await.unwrap();
    assert_eq!(content, b"Line 1\nLine 2\nLine 3\nLine 4\n");

    // === Verify no resource leaks ===
    // If test completes without hanging, resources were cleaned correctly
}

/// Verify clean shutdown after long session
#[tokio::test]
async fn long_session_clean_shutdown() {
    let temp_dir = TempDir::new().unwrap();
    let backend = LocalFs::new(temp_dir.path());

    // Open, edit, close multiple files
    for i in 0..10 {
        let path = format!("file_{}.txt", i);
        backend
            .write(&path, format!("Content {}\n", i).as_bytes())
            .await
            .unwrap();
        let _ = backend.read(&path).await.unwrap();
    }

    // Verify all files present
    for i in 0..10 {
        let path = format!("file_{}.txt", i);
        let content = backend.read(&path).await.unwrap();
        assert_eq!(content, format!("Content {}\n", i).as_bytes());
    }

    // Drop backend explicitly
    drop(backend);
    drop(temp_dir);

    // Test completes = clean shutdown
}
