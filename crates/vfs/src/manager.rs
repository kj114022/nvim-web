//! VFS Manager - Enhanced coordinator for backends, caching, and events
//!
//! Features:
//! - Backend hot-swap (switch backends without restart)
//! - LRU cache (unified caching across all backends)
//! - Lazy backend initialization
//! - File change event notifications
//! - Path aliases (@work -> vfs://ssh/server/path)

use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Result;
use tokio::sync::{broadcast, RwLock};

use super::backend::VfsBackend;

/// Cache entry with TTL tracking
#[derive(Clone)]
struct CacheEntry {
    data: Vec<u8>,
    inserted_at: std::time::Instant,
}

/// LRU cache configuration
const CACHE_MAX_ENTRIES: usize = 100;
const CACHE_TTL_SECS: u64 = 60;

/// File change event types
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VfsEvent {
    /// File was read
    Read { path: String },
    /// File was written
    Write { path: String },
    /// Backend was registered
    BackendAdded { name: String },
    /// Backend was removed
    BackendRemoved { name: String },
    /// Alias was added/updated
    AliasChanged { alias: String, target: String },
}

/// Metadata for a VFS-managed buffer
#[derive(Debug, Clone)]
pub struct ManagedBuffer {
    pub bufnr: u32,
    pub vfs_path: String,
    pub backend: String,
}

/// Backend factory for lazy initialization
pub type BackendFactory = Box<dyn Fn() -> Result<Box<dyn VfsBackend>> + Send + Sync>;

/// VFS manager - coordinates backends, caching, events, and aliases
pub struct VfsManager {
    /// Registered backends
    backends: RwLock<HashMap<String, Arc<dyn VfsBackend>>>,
    /// Lazy backend factories (for deferred initialization)
    factories: RwLock<HashMap<String, BackendFactory>>,
    /// LRU cache for read operations
    cache: RwLock<HashMap<String, CacheEntry>>,
    /// LRU order tracking
    cache_order: RwLock<Vec<String>>,
    /// Managed buffers
    managed_buffers: RwLock<HashMap<u32, ManagedBuffer>>,
    /// Path aliases (@work -> vfs://ssh/workserver/home/me)
    aliases: RwLock<HashMap<String, String>>,
    /// Event broadcast channel
    event_tx: broadcast::Sender<VfsEvent>,
}

impl Default for VfsManager {
    fn default() -> Self {
        Self::new()
    }
}

impl VfsManager {
    /// Create a new VFS manager
    pub fn new() -> Self {
        let (event_tx, _) = broadcast::channel(64);
        Self {
            backends: RwLock::new(HashMap::new()),
            factories: RwLock::new(HashMap::new()),
            cache: RwLock::new(HashMap::new()),
            cache_order: RwLock::new(Vec::new()),
            managed_buffers: RwLock::new(HashMap::new()),
            aliases: RwLock::new(HashMap::new()),
            event_tx,
        }
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Aliases
    // ─────────────────────────────────────────────────────────────────────────

    /// Register a path alias
    /// Example: add_alias("@work", "vfs://ssh/workserver:22/home/me")
    pub async fn add_alias(&self, alias: impl Into<String>, target: impl Into<String>) {
        let alias = alias.into();
        let target = target.into();
        self.aliases.write().await.insert(alias.clone(), target.clone());
        let _ = self.event_tx.send(VfsEvent::AliasChanged { alias, target });
    }

    /// Remove an alias
    pub async fn remove_alias(&self, alias: &str) {
        self.aliases.write().await.remove(alias);
    }

    /// Resolve aliases in a path
    /// "@work/src/main.rs" -> "vfs://ssh/workserver:22/home/me/src/main.rs"
    pub async fn resolve_aliases(&self, path: &str) -> String {
        let aliases = self.aliases.read().await;
        
        for (alias, target) in aliases.iter() {
            if path.starts_with(alias) {
                let suffix = &path[alias.len()..];
                let target = target.trim_end_matches('/');
                return format!("{target}{suffix}");
            }
        }
        
        path.to_string()
    }

    /// List all aliases
    pub async fn list_aliases(&self) -> Vec<(String, String)> {
        self.aliases.read().await.iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Backend Management (Hot-Swap)
    // ─────────────────────────────────────────────────────────────────────────

    /// Register a VFS backend (immediately available)
    pub async fn register_backend(&self, name: impl Into<String>, backend: Box<dyn VfsBackend>) {
        let name = name.into();
        self.backends.write().await.insert(name.clone(), Arc::from(backend));
        let _ = self.event_tx.send(VfsEvent::BackendAdded { name });
    }

    /// Register a lazy backend factory (initialized on first use)
    pub async fn register_lazy_backend(&self, name: impl Into<String>, factory: BackendFactory) {
        let name = name.into();
        self.factories.write().await.insert(name, factory);
    }

    /// Hot-swap a backend (replaces existing without restart)
    pub async fn swap_backend(&self, name: impl Into<String>, backend: Box<dyn VfsBackend>) {
        let name = name.into();
        
        // Remove from factories if it was lazy
        self.factories.write().await.remove(&name);
        
        // Replace or insert the backend
        self.backends.write().await.insert(name.clone(), Arc::from(backend));
        
        // Invalidate cache entries for this backend
        self.invalidate_backend_cache(&name).await;
        
        let _ = self.event_tx.send(VfsEvent::BackendAdded { name });
    }

    /// Remove a backend
    pub async fn remove_backend(&self, name: &str) {
        self.backends.write().await.remove(name);
        self.factories.write().await.remove(name);
        self.invalidate_backend_cache(name).await;
        let _ = self.event_tx.send(VfsEvent::BackendRemoved { name: name.to_string() });
    }

    /// Get or initialize a backend
    pub async fn get_backend(&self, name: &str) -> Result<Arc<dyn VfsBackend>> {
        // Check if already initialized
        if let Some(backend) = self.backends.read().await.get(name) {
            return Ok(backend.clone());
        }

        // Check for lazy factory
        let factory = self.factories.write().await.remove(name);
        if let Some(factory) = factory {
            let backend = factory()?;
            let arc_backend: Arc<dyn VfsBackend> = Arc::from(backend);
            self.backends.write().await.insert(name.to_string(), arc_backend.clone());
            return Ok(arc_backend);
        }

        anyhow::bail!("Unknown VFS backend: {name}")
    }

    /// List registered backends
    pub async fn list_backends(&self) -> Vec<String> {
        let mut names: Vec<String> = self.backends.read().await.keys().cloned().collect();
        names.extend(self.factories.read().await.keys().cloned());
        names.sort();
        names.dedup();
        names
    }

    // ─────────────────────────────────────────────────────────────────────────
    // LRU Cache
    // ─────────────────────────────────────────────────────────────────────────

    /// Get from cache if valid
    async fn cache_get(&self, key: &str) -> Option<Vec<u8>> {
        let cache = self.cache.read().await;
        if let Some(entry) = cache.get(key) {
            if entry.inserted_at.elapsed().as_secs() < CACHE_TTL_SECS {
                return Some(entry.data.clone());
            }
        }
        None
    }

    /// Insert into cache with LRU eviction
    async fn cache_put(&self, key: String, data: Vec<u8>) {
        let mut cache = self.cache.write().await;
        let mut order = self.cache_order.write().await;

        // Remove old entry if exists
        if let Some(pos) = order.iter().position(|k| k == &key) {
            order.remove(pos);
        }

        // Evict oldest if at capacity
        while order.len() >= CACHE_MAX_ENTRIES {
            if let Some(oldest) = order.first().cloned() {
                cache.remove(&oldest);
                order.remove(0);
            }
        }

        // Insert new entry
        cache.insert(key.clone(), CacheEntry {
            data,
            inserted_at: std::time::Instant::now(),
        });
        order.push(key);
    }

    /// Invalidate cache entry
    pub async fn cache_invalidate(&self, key: &str) {
        self.cache.write().await.remove(key);
        let mut order = self.cache_order.write().await;
        if let Some(pos) = order.iter().position(|k| k == key) {
            order.remove(pos);
        }
    }

    /// Invalidate all cache entries for a backend
    async fn invalidate_backend_cache(&self, backend: &str) {
        let prefix = format!("vfs://{backend}/");
        let mut cache = self.cache.write().await;
        let mut order = self.cache_order.write().await;
        
        let keys_to_remove: Vec<String> = cache.keys()
            .filter(|k| k.starts_with(&prefix))
            .cloned()
            .collect();
        
        for key in keys_to_remove {
            cache.remove(&key);
            if let Some(pos) = order.iter().position(|k| k == &key) {
                order.remove(pos);
            }
        }
    }

    /// Clear entire cache
    pub async fn cache_clear(&self) {
        self.cache.write().await.clear();
        self.cache_order.write().await.clear();
    }

    /// Get cache stats
    pub async fn cache_stats(&self) -> (usize, usize) {
        let cache = self.cache.read().await;
        let valid = cache.values()
            .filter(|e| e.inserted_at.elapsed().as_secs() < CACHE_TTL_SECS)
            .count();
        (cache.len(), valid)
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Events
    // ─────────────────────────────────────────────────────────────────────────

    /// Subscribe to VFS events
    pub fn subscribe(&self) -> broadcast::Receiver<VfsEvent> {
        self.event_tx.subscribe()
    }

    // ─────────────────────────────────────────────────────────────────────────
    // File Operations
    // ─────────────────────────────────────────────────────────────────────────

    /// Parse VFS path with alias resolution
    pub async fn parse_vfs_path(&self, vfs_path: &str) -> Result<(String, String)> {
        // Resolve aliases first
        let resolved = self.resolve_aliases(vfs_path).await;
        
        if !resolved.starts_with("vfs://") {
            anyhow::bail!("Invalid VFS path: must start with vfs:// (got: {vfs_path})");
        }

        let path = &resolved[6..]; // strip "vfs://"
        let parts: Vec<&str> = path.splitn(2, '/').collect();

        if parts.len() < 2 {
            anyhow::bail!("Invalid VFS path format: vfs://backend/path");
        }

        Ok((parts[0].to_string(), parts[1].to_string()))
    }

    /// Read file with caching
    pub async fn read_file(&self, vfs_path: &str) -> Result<Vec<u8>> {
        let resolved = self.resolve_aliases(vfs_path).await;
        
        // Check cache first
        if let Some(data) = self.cache_get(&resolved).await {
            return Ok(data);
        }

        let (backend_name, path) = self.parse_vfs_path(&resolved).await?;

        // SSH backend uses connection pooling
        let data = if backend_name == "ssh" {
            use super::SshFsBackend;
            let backend = SshFsBackend::get_or_connect(&resolved)?;
            let result = backend.read(&path).await;
            backend.touch();
            result?
        } else {
            let backend = self.get_backend(&backend_name).await?;
            backend.read(&path).await?
        };

        // Cache the result
        self.cache_put(resolved.clone(), data.clone()).await;
        
        // Emit event
        let _ = self.event_tx.send(VfsEvent::Read { path: resolved });

        Ok(data)
    }

    /// Write file (invalidates cache)
    pub async fn write_file(&self, vfs_path: &str, data: &[u8]) -> Result<()> {
        let resolved = self.resolve_aliases(vfs_path).await;
        let (backend_name, path) = self.parse_vfs_path(&resolved).await?;

        // Invalidate cache
        self.cache_invalidate(&resolved).await;

        // SSH backend uses connection pooling
        if backend_name == "ssh" {
            use super::SshFsBackend;
            let backend = SshFsBackend::get_or_connect(&resolved)?;
            let result = backend.write(&path, data).await;
            backend.touch();
            result?;
        } else {
            let backend = self.get_backend(&backend_name).await?;
            backend.write(&path, data).await?;
        }

        // Emit event
        let _ = self.event_tx.send(VfsEvent::Write { path: resolved });

        Ok(())
    }

    // ─────────────────────────────────────────────────────────────────────────
    // Buffer Management
    // ─────────────────────────────────────────────────────────────────────────

    /// Register a buffer as VFS-managed
    pub async fn register_buffer(&self, bufnr: u32, vfs_path: String) -> Result<()> {
        let resolved = self.resolve_aliases(&vfs_path).await;
        let (backend_name, _) = self.parse_vfs_path(&resolved).await?;

        let managed = ManagedBuffer {
            bufnr,
            vfs_path: resolved,
            backend: backend_name,
        };

        self.managed_buffers.write().await.insert(bufnr, managed);
        Ok(())
    }

    /// Get managed buffer metadata
    pub async fn get_managed_buffer(&self, bufnr: u32) -> Option<ManagedBuffer> {
        self.managed_buffers.read().await.get(&bufnr).cloned()
    }

    /// Unregister a buffer
    pub async fn unregister_buffer(&self, bufnr: u32) {
        self.managed_buffers.write().await.remove(&bufnr);
    }

    /// List all managed buffers
    pub async fn list_managed_buffers(&self) -> Vec<ManagedBuffer> {
        self.managed_buffers.read().await.values().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_alias_resolution() {
        let mgr = VfsManager::new();
        mgr.add_alias("@work", "vfs://ssh/workserver:22/home/me").await;
        
        let resolved = mgr.resolve_aliases("@work/src/main.rs").await;
        assert_eq!(resolved, "vfs://ssh/workserver:22/home/me/src/main.rs");
    }

    #[tokio::test]
    async fn test_alias_list() {
        let mgr = VfsManager::new();
        mgr.add_alias("@a", "vfs://local/a").await;
        mgr.add_alias("@b", "vfs://local/b").await;
        
        let aliases = mgr.list_aliases().await;
        assert_eq!(aliases.len(), 2);
    }

    #[tokio::test]
    async fn test_backend_list() {
        let mgr = VfsManager::new();
        
        // Register lazy factory
        mgr.register_lazy_backend("test", Box::new(|| {
            anyhow::bail!("Not implemented")
        })).await;
        
        let backends = mgr.list_backends().await;
        assert!(backends.contains(&"test".to_string()));
    }
}
