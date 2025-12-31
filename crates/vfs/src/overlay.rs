//! Overlay filesystem - layered VFS backend
//!
//! Combines multiple backends into a single layered view:
//! - Reads search layers top-to-bottom (first match wins)
//! - Writes go to the top layer only
//! - Useful for read-only base + writable overlay patterns

use std::sync::Arc;

use anyhow::{bail, Result};
use async_trait::async_trait;

use super::backend::{FileStat, ReadHandle, VfsBackend, WriteHandle};

/// Overlay filesystem combining multiple backends
///
/// Layers are ordered from bottom (index 0) to top (last index).
/// Reads are resolved top-to-bottom (first match wins).
/// Writes always go to the topmost layer.
pub struct OverlayFs {
    /// Layers ordered bottom to top (last is writeable top layer)
    layers: Vec<Arc<dyn VfsBackend>>,
}

impl OverlayFs {
    /// Create with layers (bottom to top, last is writable)
    ///
    /// # Panics
    /// Panics if layers is empty
    pub fn new(layers: Vec<Arc<dyn VfsBackend>>) -> Self {
        assert!(!layers.is_empty(), "OverlayFs requires at least one layer");
        Self { layers }
    }

    /// Create with two layers (common read-only base + writable overlay)
    pub fn two_layer(base: Arc<dyn VfsBackend>, overlay: Arc<dyn VfsBackend>) -> Self {
        Self::new(vec![base, overlay])
    }

    /// Get the top (writable) layer
    fn top_layer(&self) -> &Arc<dyn VfsBackend> {
        self.layers.last().expect("OverlayFs is not empty")
    }

    /// Iterate layers top to bottom for reads
    fn layers_top_down(&self) -> impl Iterator<Item = &Arc<dyn VfsBackend>> {
        self.layers.iter().rev()
    }
}

#[async_trait]
impl VfsBackend for OverlayFs {
    async fn read(&self, path: &str) -> Result<Vec<u8>> {
        for layer in self.layers_top_down() {
            match layer.read(path).await {
                Ok(data) => return Ok(data),
                Err(_) => continue,
            }
        }
        bail!("File not found in any layer: {path}")
    }

    async fn write(&self, path: &str, data: &[u8]) -> Result<()> {
        // Always write to top layer
        self.top_layer().write(path, data).await
    }

    async fn stat(&self, path: &str) -> Result<FileStat> {
        for layer in self.layers_top_down() {
            match layer.stat(path).await {
                Ok(stat) => return Ok(stat),
                Err(_) => continue,
            }
        }
        bail!("Not found in any layer: {path}")
    }

    async fn list(&self, path: &str) -> Result<Vec<String>> {
        // Merge listings from all layers (unique names)
        let mut all_entries = std::collections::HashSet::new();
        let mut found_dir = false;

        for layer in self.layers_top_down() {
            match layer.list(path).await {
                Ok(entries) => {
                    found_dir = true;
                    all_entries.extend(entries);
                }
                Err(_) => continue,
            }
        }

        if !found_dir {
            bail!("Directory not found in any layer: {path}")
        }

        let mut result: Vec<String> = all_entries.into_iter().collect();
        result.sort();
        Ok(result)
    }

    async fn exists(&self, path: &str) -> Result<bool> {
        for layer in self.layers_top_down() {
            if layer.exists(path).await.unwrap_or(false) {
                return Ok(true);
            }
        }
        Ok(false)
    }

    async fn create_dir(&self, path: &str) -> Result<()> {
        self.top_layer().create_dir(path).await
    }

    async fn create_dir_all(&self, path: &str) -> Result<()> {
        self.top_layer().create_dir_all(path).await
    }

    async fn remove_dir(&self, path: &str) -> Result<()> {
        // Remove from top layer only
        self.top_layer().remove_dir(path).await
    }

    async fn remove_file(&self, path: &str) -> Result<()> {
        // Remove from top layer only
        self.top_layer().remove_file(path).await
    }

    async fn copy(&self, src: &str, dest: &str) -> Result<()> {
        // Read from any layer, write to top
        let data = self.read(src).await?;
        self.write(dest, &data).await
    }

    async fn rename(&self, src: &str, dest: &str) -> Result<()> {
        // Copy then delete from top layer
        let data = self.read(src).await?;
        self.write(dest, &data).await?;
        // Only attempt delete from top layer (source might be in lower layer)
        let _ = self.top_layer().remove_file(src).await;
        Ok(())
    }

    async fn open_read(&self, path: &str) -> Result<Box<dyn ReadHandle>> {
        for layer in self.layers_top_down() {
            if layer.exists(path).await.unwrap_or(false) {
                return layer.open_read(path).await;
            }
        }
        bail!("File not found in any layer: {path}")
    }

    async fn open_write(&self, path: &str) -> Result<Box<dyn WriteHandle>> {
        self.top_layer().open_write(path).await
    }

    fn supports_streaming(&self) -> bool {
        self.top_layer().supports_streaming()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemoryFs;

    #[tokio::test]
    async fn test_read_through_layers() {
        let base: Arc<dyn VfsBackend> = Arc::new(MemoryFs::with_files(vec![
            ("/base.txt", b"base content"),
            ("/shared.txt", b"from base"),
        ]));
        let overlay: Arc<dyn VfsBackend> = Arc::new(MemoryFs::with_files(vec![
            ("/overlay.txt", b"overlay content"),
            ("/shared.txt", b"from overlay"),
        ]));
        
        let fs = OverlayFs::two_layer(base, overlay);
        
        // Read from base
        assert_eq!(fs.read("/base.txt").await.unwrap(), b"base content");
        
        // Read from overlay
        assert_eq!(fs.read("/overlay.txt").await.unwrap(), b"overlay content");
        
        // Overlay wins for shared
        assert_eq!(fs.read("/shared.txt").await.unwrap(), b"from overlay");
    }

    #[tokio::test]
    async fn test_write_to_top() {
        let base = Arc::new(MemoryFs::with_files(vec![("/readonly.txt", b"base")]));
        let overlay = Arc::new(MemoryFs::new());
        
        let base_dyn: Arc<dyn VfsBackend> = Arc::clone(&base) as Arc<dyn VfsBackend>;
        let overlay_dyn: Arc<dyn VfsBackend> = Arc::clone(&overlay) as Arc<dyn VfsBackend>;
        
        let fs = OverlayFs::two_layer(base_dyn, overlay_dyn);
        
        // Write new file
        fs.write("/new.txt", b"new content").await.unwrap();
        
        // Should be readable
        assert_eq!(fs.read("/new.txt").await.unwrap(), b"new content");
        
        // Should be in overlay, not base
        assert!(overlay.exists("/new.txt").await.unwrap());
        assert!(!base.exists("/new.txt").await.unwrap());
    }

    #[tokio::test]
    async fn test_merged_listing() {
        let base: Arc<dyn VfsBackend> = Arc::new(MemoryFs::with_files(vec![
            ("/dir/a.txt", b"a"),
            ("/dir/b.txt", b"b"),
        ]));
        let overlay: Arc<dyn VfsBackend> = Arc::new(MemoryFs::with_files(vec![
            ("/dir/c.txt", b"c"),
        ]));
        
        let fs = OverlayFs::two_layer(base, overlay);
        
        let entries = fs.list("/dir").await.unwrap();
        assert_eq!(entries, vec!["a.txt", "b.txt", "c.txt"]);
    }
}
