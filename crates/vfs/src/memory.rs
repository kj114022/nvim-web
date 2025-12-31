//! In-memory filesystem backend for testing
//!
//! Provides a fast, ephemeral filesystem that exists only in memory.
//! Useful for unit tests that need VFS operations without disk I/O.

use std::collections::HashMap;
use std::sync::{Arc, RwLock};

use anyhow::{bail, Result};
use async_trait::async_trait;

use super::backend::{FileStat, ReadChunk, ReadHandle, VfsBackend, WriteHandle};

/// In-memory file entry
#[derive(Clone, Debug)]
enum MemoryEntry {
    File(Vec<u8>),
    Directory,
}

/// In-memory filesystem backend
///
/// All data is stored in memory and lost when the backend is dropped.
/// Thread-safe via internal RwLock.
pub struct MemoryFs {
    entries: Arc<RwLock<HashMap<String, MemoryEntry>>>,
}

impl Default for MemoryFs {
    fn default() -> Self {
        Self::new()
    }
}

impl MemoryFs {
    /// Create a new empty in-memory filesystem
    pub fn new() -> Self {
        let mut entries = HashMap::new();
        // Root always exists
        entries.insert("/".to_string(), MemoryEntry::Directory);
        Self {
            entries: Arc::new(RwLock::new(entries)),
        }
    }

    /// Create with initial file contents
    pub fn with_files(files: Vec<(&str, &[u8])>) -> Self {
        let fs = Self::new();
        {
            let mut entries = fs.entries.write().unwrap();
            for (path, content) in files {
                let path = Self::normalize_path(path);
                // Create parent directories
                let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
                let mut current = String::new();
                for part in &parts[..parts.len().saturating_sub(1)] {
                    current = format!("{current}/{part}");
                    entries.entry(current.clone()).or_insert(MemoryEntry::Directory);
                }
                entries.insert(path, MemoryEntry::File(content.to_vec()));
            }
        }
        fs
    }

    /// Normalize path (ensure leading /, no trailing /)
    fn normalize_path(path: &str) -> String {
        let path = path.trim();
        if path.is_empty() || path == "/" {
            return "/".to_string();
        }
        let path = if path.starts_with('/') { 
            path.to_string() 
        } else { 
            format!("/{path}") 
        };
        path.trim_end_matches('/').to_string()
    }

    /// Get parent path
    fn parent_path(path: &str) -> Option<String> {
        let path = Self::normalize_path(path);
        if path == "/" {
            return None;
        }
        let idx = path.rfind('/')?;
        if idx == 0 {
            Some("/".to_string())
        } else {
            Some(path[..idx].to_string())
        }
    }

    /// Get file name from path
    fn file_name(path: &str) -> Option<String> {
        let path = Self::normalize_path(path);
        path.rsplit('/').next().map(String::from)
    }

    /// Ensure parent directories exist
    fn ensure_parents(&self, path: &str) -> Result<()> {
        let path = Self::normalize_path(path);
        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        let mut entries = self.entries.write().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        
        let mut current = String::new();
        for part in &parts[..parts.len().saturating_sub(1)] {
            current = format!("{current}/{part}");
            entries.entry(current.clone()).or_insert(MemoryEntry::Directory);
        }
        Ok(())
    }
}

#[async_trait]
impl VfsBackend for MemoryFs {
    async fn read(&self, path: &str) -> Result<Vec<u8>> {
        let path = Self::normalize_path(path);
        let entries = self.entries.read().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        
        match entries.get(&path) {
            Some(MemoryEntry::File(data)) => Ok(data.clone()),
            Some(MemoryEntry::Directory) => bail!("Cannot read directory: {path}"),
            None => bail!("File not found: {path}"),
        }
    }

    async fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        let path = Self::normalize_path(path);
        self.ensure_parents(&path)?;
        
        let mut entries = self.entries.write().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        entries.insert(path, MemoryEntry::File(data.to_vec()));
        Ok(())
    }

    async fn stat(&self, path: &str) -> Result<FileStat> {
        let path = Self::normalize_path(path);
        let entries = self.entries.read().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        
        match entries.get(&path) {
            Some(MemoryEntry::File(data)) => Ok(FileStat::file(data.len() as u64)),
            Some(MemoryEntry::Directory) => Ok(FileStat::dir()),
            None => bail!("Not found: {path}"),
        }
    }

    async fn list(&self, path: &str) -> Result<Vec<String>> {
        let path = Self::normalize_path(path);
        let entries = self.entries.read().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        
        // Verify path is a directory
        match entries.get(&path) {
            Some(MemoryEntry::Directory) => {}
            Some(MemoryEntry::File(_)) => bail!("Not a directory: {path}"),
            None => bail!("Directory not found: {path}"),
        }

        let prefix = if path == "/" { "/" } else { &format!("{path}/") };
        let mut results = Vec::new();
        
        for key in entries.keys() {
            if key.starts_with(prefix) && key != &path {
                let remainder = &key[prefix.len()..];
                // Only direct children (no / in remainder)
                if !remainder.contains('/') && !remainder.is_empty() {
                    results.push(remainder.to_string());
                }
            }
        }
        
        results.sort();
        Ok(results)
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        let path = Self::normalize_path(path);
        let entries = self.entries.read().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        Ok(entries.contains_key(&path))
    }

    async fn create_dir(&self, path: &str) -> Result<()> {
        let path = Self::normalize_path(path);
        
        // Check parent exists
        if let Some(parent) = Self::parent_path(&path) {
            let entries = self.entries.read().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
            if !entries.contains_key(&parent) {
                bail!("Parent directory does not exist: {parent}");
            }
        }
        
        let mut entries = self.entries.write().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        if entries.contains_key(&path) {
            bail!("Already exists: {path}");
        }
        entries.insert(path, MemoryEntry::Directory);
        Ok(())
    }

    async fn create_dir_all(&self, path: &str) -> Result<()> {
        let path = Self::normalize_path(path);
        self.ensure_parents(&path)?;
        
        let mut entries = self.entries.write().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        entries.entry(path).or_insert(MemoryEntry::Directory);
        Ok(())
    }

    async fn remove_dir(&self, path: &str) -> Result<()> {
        let path = Self::normalize_path(path);
        
        // Check if empty
        let entries = self.entries.read().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        let prefix = format!("{path}/");
        for key in entries.keys() {
            if key.starts_with(&prefix) {
                bail!("Directory not empty: {path}");
            }
        }
        drop(entries);
        
        let mut entries = self.entries.write().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        match entries.remove(&path) {
            Some(MemoryEntry::Directory) => Ok(()),
            Some(_) => bail!("Not a directory: {path}"),
            None => bail!("Directory not found: {path}"),
        }
    }

    async fn remove_file(&self, path: &str) -> Result<()> {
        let path = Self::normalize_path(path);
        let mut entries = self.entries.write().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        match entries.remove(&path) {
            Some(MemoryEntry::File(_)) => Ok(()),
            Some(_) => bail!("Not a file: {path}"),
            None => bail!("File not found: {path}"),
        }
    }

    async fn copy(&self, src: &str, dest: &str) -> Result<()> {
        let src = Self::normalize_path(src);
        let dest = Self::normalize_path(dest);
        
        let data = {
            let entries = self.entries.read().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
            match entries.get(&src) {
                Some(MemoryEntry::File(data)) => data.clone(),
                Some(MemoryEntry::Directory) => bail!("Cannot copy directory: {src}"),
                None => bail!("File not found: {src}"),
            }
        };
        
        self.ensure_parents(&dest)?;
        let mut entries = self.entries.write().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        entries.insert(dest, MemoryEntry::File(data));
        Ok(())
    }

    async fn rename(&self, src: &str, dest: &str) -> Result<()> {
        let src = Self::normalize_path(src);
        let dest = Self::normalize_path(dest);
        
        self.ensure_parents(&dest)?;
        
        let mut entries = self.entries.write().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        let entry = entries.remove(&src).ok_or_else(|| anyhow::anyhow!("Not found: {src}"))?;
        entries.insert(dest, entry);
        Ok(())
    }

    async fn open_read(&self, path: &str) -> Result<Box<dyn ReadHandle>> {
        let data = self.read(path).await?;
        Ok(Box::new(MemoryReadHandle::new(data)))
    }

    async fn open_write(&self, path: &str) -> Result<Box<dyn WriteHandle>> {
        let path = Self::normalize_path(path);
        self.ensure_parents(&path)?;
        Ok(Box::new(MemoryWriteHandle::new(path, self.entries.clone())))
    }

    fn supports_streaming(&self) -> bool {
        true
    }
}

/// In-memory read handle
struct MemoryReadHandle {
    data: Vec<u8>,
    offset: usize,
}

impl MemoryReadHandle {
    fn new(data: Vec<u8>) -> Self {
        Self { data, offset: 0 }
    }
}

#[async_trait]
impl ReadHandle for MemoryReadHandle {
    async fn read_chunk(&mut self) -> Result<ReadChunk> {
        const CHUNK_SIZE: usize = 64 * 1024;
        let remaining = self.data.len().saturating_sub(self.offset);
        let chunk_size = remaining.min(CHUNK_SIZE);
        
        let chunk = ReadChunk {
            data: self.data[self.offset..self.offset + chunk_size].to_vec(),
            offset: self.offset as u64,
            is_last: self.offset + chunk_size >= self.data.len(),
        };
        self.offset += chunk_size;
        Ok(chunk)
    }

    fn size(&self) -> Option<u64> {
        Some(self.data.len() as u64)
    }

    async fn close(&mut self) -> Result<()> {
        Ok(())
    }
}

/// In-memory write handle
struct MemoryWriteHandle {
    path: String,
    buffer: Vec<u8>,
    entries: Arc<RwLock<HashMap<String, MemoryEntry>>>,
}

impl MemoryWriteHandle {
    fn new(path: String, entries: Arc<RwLock<HashMap<String, MemoryEntry>>>) -> Self {
        Self {
            path,
            buffer: Vec::new(),
            entries,
        }
    }
}

#[async_trait]
impl WriteHandle for MemoryWriteHandle {
    async fn write_chunk(&mut self, data: &[u8]) -> Result<()> {
        self.buffer.extend_from_slice(data);
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        let mut entries = self.entries.write().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
        entries.insert(self.path.clone(), MemoryEntry::File(std::mem::take(&mut self.buffer)));
        Ok(())
    }

    fn bytes_written(&self) -> u64 {
        self.buffer.len() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_operations() {
        let fs = MemoryFs::new();
        
        // Write
        fs.write("/test.txt", b"Hello").await.unwrap();
        
        // Read
        let data = fs.read("/test.txt").await.unwrap();
        assert_eq!(data, b"Hello");
        
        // Stat
        let stat = fs.stat("/test.txt").await.unwrap();
        assert!(stat.is_file);
        assert_eq!(stat.size, 5);
        
        // Exists
        assert!(fs.exists("/test.txt").await.unwrap());
        assert!(!fs.exists("/nonexistent").await.unwrap());
    }

    #[tokio::test]
    async fn test_directory_operations() {
        let fs = MemoryFs::new();
        
        // Create directory
        fs.create_dir("/mydir").await.unwrap();
        
        // Write file in directory
        fs.write("/mydir/file.txt", b"content").await.unwrap();
        
        // List directory
        let entries = fs.list("/mydir").await.unwrap();
        assert_eq!(entries, vec!["file.txt"]);
        
        // List root
        let root = fs.list("/").await.unwrap();
        assert!(root.contains(&"mydir".to_string()));
    }

    #[tokio::test]
    async fn test_streaming() {
        let fs = MemoryFs::new();
        
        // Write via streaming
        {
            let mut writer = fs.open_write("/stream.txt").await.unwrap();
            writer.write_chunk(b"Hello, ").await.unwrap();
            writer.write_chunk(b"World!").await.unwrap();
            writer.close().await.unwrap();
        }
        
        // Read via streaming
        {
            let mut reader = fs.open_read("/stream.txt").await.unwrap();
            assert_eq!(reader.size(), Some(13));
            
            let chunk = reader.read_chunk().await.unwrap();
            assert_eq!(chunk.data, b"Hello, World!");
            assert!(chunk.is_last);
        }
    }

    #[tokio::test]
    async fn test_with_files() {
        let fs = MemoryFs::with_files(vec![
            ("/a.txt", b"A"),
            ("/dir/b.txt", b"B"),
            ("/dir/sub/c.txt", b"C"),
        ]);
        
        assert_eq!(fs.read("/a.txt").await.unwrap(), b"A");
        assert_eq!(fs.read("/dir/b.txt").await.unwrap(), b"B");
        assert_eq!(fs.read("/dir/sub/c.txt").await.unwrap(), b"C");
    }
}
