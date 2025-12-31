use std::fs;
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{bail, Result};
use async_trait::async_trait;

use super::backend::{FileStat, ReadChunk, ReadHandle, VfsBackend, WriteHandle};

/// Default chunk size for streaming (64KB)
const CHUNK_SIZE: usize = 64 * 1024;

/// Local filesystem backend - maps `<vfs://local/...>` to real filesystem
pub struct LocalFs {
    root: PathBuf,
}

impl LocalFs {
    /// Create new local FS backend with specified root directory
    pub fn new(root: impl Into<PathBuf>) -> Self {
        let root_path = root.into();
        // Ensure root exists and canonicalize it
        let _ = fs::create_dir_all(&root_path);
        Self {
            root: root_path.canonicalize().unwrap_or(root_path),
        }
    }

    /// Resolve VFS path to absolute filesystem path with security checks
    ///
    /// SECURITY: Prevents path traversal attacks by:
    /// 1. Canonicalizing the resolved path
    /// 2. Verifying it stays within the sandbox root
    fn resolve(&self, path: &str) -> Result<PathBuf> {
        // Build the target path
        let target = self.root.join(path.trim_start_matches('/'));

        // For read/stat operations, canonicalize to get real path
        // For write operations, parent must exist and be within sandbox
        let resolved = if target.exists() {
            target.canonicalize()?
        } else {
            // For non-existent files, check parent directory
            let parent = target
                .parent()
                .ok_or_else(|| anyhow::anyhow!("Invalid path: no parent"))?;

            // Create parent if needed, then canonicalize
            fs::create_dir_all(parent)?;
            let canonical_parent = parent.canonicalize()?;

            // Reconstruct path with canonical parent
            canonical_parent.join(
                target
                    .file_name()
                    .ok_or_else(|| anyhow::anyhow!("Invalid path: no filename"))?,
            )
        };

        // SECURITY CHECK: Verify path is within sandbox
        if !resolved.starts_with(&self.root) {
            bail!(
                "Path traversal blocked: {} escapes sandbox {}",
                path,
                self.root.display()
            );
        }

        Ok(resolved)
    }

    /// Resolve path without creating parent directories (for read/stat operations)
    fn resolve_existing(&self, path: &str) -> Result<PathBuf> {
        let target = self.root.join(path.trim_start_matches('/'));
        let resolved = target.canonicalize()?;

        if !resolved.starts_with(&self.root) {
            bail!(
                "Path traversal blocked: {} escapes sandbox {}",
                path,
                self.root.display()
            );
        }

        Ok(resolved)
    }
}

#[async_trait]
impl VfsBackend for LocalFs {
    async fn read(&self, path: &str) -> Result<Vec<u8>> {
        let resolved = self.resolve(path)?;
        tokio::task::spawn_blocking(move || fs::read(resolved).map_err(Into::into)).await?
    }

    async fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        let resolved = self.resolve(path)?;
        let data = data.to_vec();
        tokio::task::spawn_blocking(move || fs::write(resolved, data).map_err(Into::into)).await?
    }

    async fn stat(&self, path: &str) -> Result<FileStat> {
        let resolved = self.resolve(path)?;
        tokio::task::spawn_blocking(move || {
            let meta = fs::metadata(&resolved)?;
            Ok(FileStat {
                is_file: meta.is_file(),
                is_dir: meta.is_dir(),
                size: meta.len(),
                created: meta.created().ok(),
                modified: meta.modified().ok(),
                readonly: meta.permissions().readonly(),
            })
        })
        .await?
    }

    async fn list(&self, path: &str) -> Result<Vec<String>> {
        let resolved = self.resolve(path)?;
        tokio::task::spawn_blocking(move || {
            let mut entries = Vec::new();
            for entry in fs::read_dir(resolved)? {
                let entry = entry?;
                entries.push(entry.file_name().to_string_lossy().into_owned());
            }
            Ok(entries)
        })
        .await?
    }

    async fn create_dir(&self, path: &str) -> Result<()> {
        let resolved = self.resolve(path)?;
        tokio::task::spawn_blocking(move || fs::create_dir(resolved).map_err(Into::into)).await?
    }

    async fn create_dir_all(&self, path: &str) -> Result<()> {
        let resolved = self.resolve(path)?;
        tokio::task::spawn_blocking(move || fs::create_dir_all(resolved).map_err(Into::into))
            .await?
    }

    async fn remove_dir(&self, path: &str) -> Result<()> {
        let resolved = self.resolve_existing(path)?;
        tokio::task::spawn_blocking(move || fs::remove_dir(resolved).map_err(Into::into)).await?
    }

    async fn remove_file(&self, path: &str) -> Result<()> {
        let resolved = self.resolve_existing(path)?;
        tokio::task::spawn_blocking(move || fs::remove_file(resolved).map_err(Into::into)).await?
    }

    async fn copy(&self, src: &str, dest: &str) -> Result<()> {
        let src_resolved = self.resolve_existing(src)?;
        let dest_resolved = self.resolve(dest)?;
        tokio::task::spawn_blocking(move || {
            fs::copy(src_resolved, dest_resolved)?;
            Ok(())
        })
        .await?
    }

    async fn rename(&self, src: &str, dest: &str) -> Result<()> {
        let src_resolved = self.resolve_existing(src)?;
        let dest_resolved = self.resolve(dest)?;
        tokio::task::spawn_blocking(move || fs::rename(src_resolved, dest_resolved).map_err(Into::into))
            .await?
    }

    async fn open_read(&self, path: &str) -> Result<Box<dyn ReadHandle>> {
        let resolved = self.resolve_existing(path)?;
        let handle = tokio::task::spawn_blocking(move || {
            FileReadHandle::new(resolved)
        }).await??;
        Ok(Box::new(handle))
    }

    async fn open_write(&self, path: &str) -> Result<Box<dyn WriteHandle>> {
        let resolved = self.resolve(path)?;
        let handle = tokio::task::spawn_blocking(move || {
            FileWriteHandle::new(resolved)
        }).await??;
        Ok(Box::new(handle))
    }

    fn supports_streaming(&self) -> bool {
        true
    }
}

/// Streaming read handle for local files
pub struct FileReadHandle {
    reader: Arc<Mutex<BufReader<File>>>,
    size: u64,
    offset: u64,
}

impl FileReadHandle {
    fn new(path: PathBuf) -> Result<Self> {
        let file = File::open(&path)?;
        let size = file.metadata()?.len();
        Ok(Self {
            reader: Arc::new(Mutex::new(BufReader::with_capacity(CHUNK_SIZE, file))),
            size,
            offset: 0,
        })
    }
}

#[async_trait]
impl ReadHandle for FileReadHandle {
    async fn read_chunk(&mut self) -> Result<ReadChunk> {
        let reader = self.reader.clone();
        let current_offset = self.offset;
        let remaining = self.size.saturating_sub(current_offset);
        
        let chunk = tokio::task::spawn_blocking(move || {
            let mut guard = reader.lock().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
            let mut buffer = vec![0u8; CHUNK_SIZE.min(remaining as usize)];
            let bytes_read = guard.read(&mut buffer)?;
            buffer.truncate(bytes_read);
            Ok::<_, anyhow::Error>(buffer)
        }).await??;

        let bytes_read = chunk.len() as u64;
        let chunk = ReadChunk {
            data: chunk,
            offset: self.offset,
            is_last: self.offset + bytes_read >= self.size,
        };
        self.offset += bytes_read;
        Ok(chunk)
    }

    fn size(&self) -> Option<u64> {
        Some(self.size)
    }

    async fn close(&mut self) -> Result<()> {
        // Drop happens automatically when Arc count reaches 0
        Ok(())
    }
}

/// Streaming write handle for local files
pub struct FileWriteHandle {
    writer: Arc<Mutex<BufWriter<File>>>,
    bytes_written: u64,
}

impl FileWriteHandle {
    fn new(path: PathBuf) -> Result<Self> {
        let file = File::create(&path)?;
        Ok(Self {
            writer: Arc::new(Mutex::new(BufWriter::with_capacity(CHUNK_SIZE, file))),
            bytes_written: 0,
        })
    }
}

#[async_trait]
impl WriteHandle for FileWriteHandle {
    async fn write_chunk(&mut self, data: &[u8]) -> Result<()> {
        let writer = self.writer.clone();
        let data = data.to_vec();
        let bytes = data.len() as u64;
        
        tokio::task::spawn_blocking(move || {
            let mut guard = writer.lock().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
            guard.write_all(&data)?;
            Ok::<_, anyhow::Error>(())
        }).await??;
        
        self.bytes_written += bytes;
        Ok(())
    }

    async fn close(&mut self) -> Result<()> {
        let writer = self.writer.clone();
        tokio::task::spawn_blocking(move || {
            let mut guard = writer.lock().map_err(|_| anyhow::anyhow!("Lock poisoned"))?;
            guard.flush()?;
            Ok::<_, anyhow::Error>(())
        }).await??;
        Ok(())
    }

    fn bytes_written(&self) -> u64 {
        self.bytes_written
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_streaming_read_write() {
        let dir = tempdir().unwrap();
        let fs = LocalFs::new(dir.path());
        
        // Write some data using streaming
        let test_data = b"Hello, streaming world!";
        {
            let mut writer = fs.open_write("test.txt").await.unwrap();
            writer.write_chunk(test_data).await.unwrap();
            writer.close().await.unwrap();
            assert_eq!(writer.bytes_written(), test_data.len() as u64);
        }
        
        // Read it back using streaming
        {
            let mut reader = fs.open_read("test.txt").await.unwrap();
            assert_eq!(reader.size(), Some(test_data.len() as u64));
            
            let chunk = reader.read_chunk().await.unwrap();
            assert_eq!(chunk.data, test_data);
            assert!(chunk.is_last);
            reader.close().await.unwrap();
        }
    }

    #[tokio::test]
    async fn test_large_file_streaming() {
        let dir = tempdir().unwrap();
        let fs = LocalFs::new(dir.path());
        
        // Write 256KB in chunks
        let chunk_count = 4;
        let chunk_data = vec![0xABu8; CHUNK_SIZE];
        {
            let mut writer = fs.open_write("large.bin").await.unwrap();
            for _ in 0..chunk_count {
                writer.write_chunk(&chunk_data).await.unwrap();
            }
            writer.close().await.unwrap();
            assert_eq!(writer.bytes_written(), (CHUNK_SIZE * chunk_count) as u64);
        }
        
        // Read back and verify
        {
            let mut reader = fs.open_read("large.bin").await.unwrap();
            assert_eq!(reader.size(), Some((CHUNK_SIZE * chunk_count) as u64));
            
            let mut total_read = 0;
            loop {
                let chunk = reader.read_chunk().await.unwrap();
                total_read += chunk.data.len();
                if chunk.is_last {
                    break;
                }
            }
            assert_eq!(total_read, CHUNK_SIZE * chunk_count);
        }
    }
}

