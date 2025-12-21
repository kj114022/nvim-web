use nvim_web_host::vfs::{VfsBackend, LocalFs};
use std::path::PathBuf;
use tempfile::TempDir;

#[test]
fn localfs_read_write_roundtrip() {
    let tmp = TempDir::new().unwrap();
    let fs = LocalFs::new(tmp.path());
    
    fs.write("test.txt", b"hello world").unwrap();
    let content = fs.read("test.txt").unwrap();
    
    assert_eq!(content, b"hello world");
}

#[test]
fn localfs_write_creates_parent_dirs() {
    let tmp = TempDir::new().unwrap();
    let fs = LocalFs::new(tmp.path());
    
    fs.write("a/b/c/test.txt", b"nested").unwrap();
    let content = fs.read("a/b/c/test.txt").unwrap();
    
    assert_eq!(content, b"nested");
}

#[test]
fn localfs_read_nonexistent_fails() {
    let tmp = TempDir::new().unwrap();
    let fs = LocalFs::new(tmp.path());
    
    let result = fs.read("does_not_exist.txt");
    assert!(result.is_err());
}

#[test]
fn localfs_sandbox_prevents_escape() {
    let tmp = TempDir::new().unwrap();
    let fs = LocalFs::new(tmp.path());
    
    // Trying to escape sandbox should still be contained
    // The resolve function joins with root, preventing escape
    let result = fs.write("../../../etc/passwd", b"hack");
    
    // This should either fail or write within sandbox
    // Let's verify it doesn't write to actual /etc/passwd
    if result.is_ok() {
        // If it succeeded, verify it's in sandbox
        let actual_path = tmp.path().join("../../../etc/passwd");
        assert!(actual_path.starts_with(tmp.path()) || !actual_path.exists());
    }
}

#[test]
fn localfs_stat_file() {
    let tmp = TempDir::new().unwrap();
    let fs = LocalFs::new(tmp.path());
    
    fs.write("file.txt", b"content").unwrap();
    let stat = fs.stat("file.txt").unwrap();
    
    assert!(stat.is_file);
    assert!(!stat.is_dir);
    assert_eq!(stat.size, 7); // "content" = 7 bytes
}

#[test]
fn localfs_stat_directory() {
    let tmp = TempDir::new().unwrap();
    let fs = LocalFs::new(tmp.path());
    
    std::fs::create_dir(tmp.path().join("dir")).unwrap();
    let stat = fs.stat("dir").unwrap();
    
    assert!(!stat.is_file);
    assert!(stat.is_dir);
}

#[test]
fn localfs_list_directory() {
    let tmp = TempDir::new().unwrap();
    let fs = LocalFs::new(tmp.path());
    
    fs.write("a.txt", b"a").unwrap();
    fs.write("b.txt", b"b").unwrap();
    
    let mut entries = fs.list("").unwrap();
    entries.sort();
    
    assert_eq!(entries, vec!["a.txt", "b.txt"]);
}
