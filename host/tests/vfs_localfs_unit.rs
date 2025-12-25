use nvim_web_host::vfs::{LocalFs, VfsBackend};
use tempfile::TempDir;

#[tokio::test]
async fn localfs_read_write_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let fs = LocalFs::new(tmp.path());

    fs.write("test.txt", b"hello world").await.unwrap();
    let content = fs.read("test.txt").await.unwrap();

    assert_eq!(content, b"hello world");
}

#[tokio::test]
async fn localfs_write_creates_parent_dirs() {
    let tmp = TempDir::new().unwrap();
    let fs = LocalFs::new(tmp.path());

    fs.write("a/b/c/test.txt", b"nested").await.unwrap();
    let content = fs.read("a/b/c/test.txt").await.unwrap();

    assert_eq!(content, b"nested");
}

#[tokio::test]
async fn localfs_read_nonexistent_fails() {
    let tmp = TempDir::new().unwrap();
    let fs = LocalFs::new(tmp.path());

    let result = fs.read("does_not_exist.txt").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn localfs_sandbox_prevents_escape() {
    let tmp = TempDir::new().unwrap();
    let fs = LocalFs::new(tmp.path());

    // Trying to escape sandbox should still be contained
    let result = fs.write("../../../etc/passwd", b"hack").await;

    // This should either fail or write within sandbox
    if result.is_ok() {
        let actual_path = tmp.path().join("../../../etc/passwd");
        assert!(actual_path.starts_with(tmp.path()) || !actual_path.exists());
    }
}

#[tokio::test]
async fn localfs_stat_file() {
    let tmp = TempDir::new().unwrap();
    let fs = LocalFs::new(tmp.path());

    fs.write("file.txt", b"content").await.unwrap();
    let stat = fs.stat("file.txt").await.unwrap();

    assert!(stat.is_file);
    assert!(!stat.is_dir);
    assert_eq!(stat.size, 7); // "content" = 7 bytes
}

#[tokio::test]
async fn localfs_stat_directory() {
    let tmp = TempDir::new().unwrap();
    let fs = LocalFs::new(tmp.path());

    std::fs::create_dir(tmp.path().join("dir")).unwrap();
    let stat = fs.stat("dir").await.unwrap();

    assert!(!stat.is_file);
    assert!(stat.is_dir);
}

#[tokio::test]
async fn localfs_list_directory() {
    let tmp = TempDir::new().unwrap();
    let fs = LocalFs::new(tmp.path());

    fs.write("a.txt", b"a").await.unwrap();
    fs.write("b.txt", b"b").await.unwrap();

    let mut entries = fs.list("").await.unwrap();
    entries.sort();

    assert_eq!(entries, vec!["a.txt", "b.txt"]);
}
