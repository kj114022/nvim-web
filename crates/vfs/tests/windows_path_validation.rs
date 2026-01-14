use anyhow::Result;
use nvim_web_vfs::{LocalFs, VfsBackend};
use tempfile::tempdir;

#[tokio::test]
async fn test_path_validation_basics() -> Result<()> {
    let dir = tempdir()?;
    let fs = LocalFs::new(dir.path());

    // Test rejection of backslashes
    let result = fs.read("foo\\bar.txt").await;
    assert!(result.is_err(), "Backslashes should be rejected");
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("backslashes not allowed"));

    // Test rejection of colons
    let result = fs.read("data:stream").await;
    assert!(result.is_err(), "Colons should be rejected");
    assert!(result
        .unwrap_err()
        .to_string()
        .contains("colon not allowed"));

    // Normal paths working
    fs.write("valid.txt", b"ok").await?;
    assert!(fs.read("valid.txt").await.is_ok());

    Ok(())
}

#[tokio::test]
async fn test_windows_traversal_attempts() -> Result<()> {
    let dir = tempdir()?;
    let fs = LocalFs::new(dir.path());

    // These attempts use backslashes so they should fail validation before even hitting resolver
    let result = fs.read("..\\..\\Windows\\System32").await;
    assert!(result.is_err());

    // Test canonicalization bypass attempts with forward slashes
    // This resolves to outside root, should be blocked by verify_sandbox
    let result = fs.read("../../../etc/passwd").await;
    assert!(result.is_err());
    // Error message might vary by OS between "invalid path" (no parent) or "escapes sandbox"
    // But it MUST be an error.

    Ok(())
}
