//! Async session management using nvim-rs with tokio
//!
//! Each session owns a Neovim instance via nvim-rs and broadcasts
//! redraw events to connected WebSocket clients.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;
use nvim_rs::{Handler, Neovim, Value};
use nvim_rs::compat::tokio::Compat;
use nvim_rs::create::tokio::new_child_cmd;
use tokio::process::{Command, ChildStdin};
use tokio::sync::broadcast;

/// Unique session identifier
pub type SessionId = String;

/// Generate a new unique session ID
pub fn generate_session_id() -> SessionId {
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    format!("{:x}", now)
}

/// The writer type used by nvim-rs with tokio
pub type NvimWriter = Compat<ChildStdin>;

/// Handler for Neovim notifications - forwards redraw events to broadcast channel
#[derive(Clone)]
pub struct RedrawHandler {
    redraw_tx: broadcast::Sender<Vec<u8>>,
    session_id: String,
}

impl RedrawHandler {
    pub fn new(session_id: String, redraw_tx: broadcast::Sender<Vec<u8>>) -> Self {
        Self { redraw_tx, session_id }
    }
}

#[async_trait]
impl Handler for RedrawHandler {
    type Writer = NvimWriter;

    async fn handle_notify(
        &self,
        name: String,
        args: Vec<Value>,
        _neovim: Neovim<Self::Writer>,
    ) {
        if name == "redraw" {
            // Encode the redraw notification as msgpack for the browser
            // Format: [2, "redraw", [...events...]]
            let msg = Value::Array(vec![
                Value::Integer(2.into()),
                Value::String("redraw".into()),
                Value::Array(args),
            ]);
            
            let mut bytes = Vec::new();
            if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
                // Broadcast to all connected clients for this session
                let _ = self.redraw_tx.send(bytes);
            }
        }
    }
}

/// A single async Neovim session
pub struct AsyncSession {
    pub id: SessionId,
    pub nvim: Neovim<NvimWriter>,
    pub redraw_tx: broadcast::Sender<Vec<u8>>,
    pub created_at: Instant,
    pub last_active: Instant,
    pub connected: bool,
}

impl AsyncSession {
    /// Create a new session with a freshly spawned Neovim instance
    pub async fn new() -> Result<Self> {
        let id = generate_session_id();
        let id_for_log = id.clone();
        
        // Create broadcast channel for redraw events
        let (redraw_tx, _) = broadcast::channel::<Vec<u8>>(256);
        
        // Create handler that forwards redraws
        let handler = RedrawHandler::new(id.clone(), redraw_tx.clone());
        
        // Spawn neovim using nvim-rs
        let mut cmd = Command::new("nvim");
        cmd.arg("--embed")
           .arg("--headless");
        
        let (nvim, io_handler, _child) = new_child_cmd(&mut cmd, handler).await?;
        
        // Spawn the IO handler task
        tokio::spawn(async move {
            if let Err(e) = io_handler.await {
                eprintln!("SESSION {}: IO handler error: {:?}", id_for_log, e);
            }
        });
        
        // Attach UI with ext_linegrid
        let mut opts = nvim_rs::UiAttachOptions::default();
        opts.set_linegrid_external(true);
        nvim.ui_attach(80, 24, &opts).await?;
        
        eprintln!("SESSION: Created new async session {}", id);
        
        let now = Instant::now();
        Ok(Self {
            id,
            nvim,
            redraw_tx,
            created_at: now,
            last_active: now,
            connected: false,
        })
    }
    
    /// Mark this session as active
    pub fn touch(&mut self) {
        self.last_active = Instant::now();
    }
    
    /// Get a receiver for redraw events
    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.redraw_tx.subscribe()
    }
    
    /// Send input to Neovim
    pub async fn input(&self, keys: &str) -> Result<()> {
        self.nvim.input(keys).await?;
        Ok(())
    }
    
    /// Resize the UI
    pub async fn resize(&self, width: i64, height: i64) -> Result<()> {
        self.nvim.ui_try_resize(width, height).await?;
        Ok(())
    }
    
    /// Request a full redraw (for reconnection)
    pub async fn request_redraw(&self) -> Result<()> {
        // Trigger redraw via command
        self.nvim.command("redraw!").await?;
        Ok(())
    }
}

/// Async manager for multiple Neovim sessions
pub struct AsyncSessionManager {
    sessions: HashMap<SessionId, AsyncSession>,
    pub timeout: Duration,
}

impl AsyncSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            timeout: Duration::from_secs(300), // 5 minutes
        }
    }
    
    /// Create a new session and return its ID
    pub async fn create_session(&mut self) -> Result<SessionId> {
        let session = AsyncSession::new().await?;
        let id = session.id.clone();
        self.sessions.insert(id.clone(), session);
        Ok(id)
    }
    
    /// Get a mutable reference to a session
    pub fn get_session_mut(&mut self, id: &str) -> Option<&mut AsyncSession> {
        self.sessions.get_mut(id)
    }
    
    /// Get an immutable reference to a session
    pub fn get_session(&self, id: &str) -> Option<&AsyncSession> {
        self.sessions.get(id)
    }
    
    /// Check if a session exists
    pub fn has_session(&self, id: &str) -> bool {
        self.sessions.contains_key(id)
    }
    
    /// Remove a session
    pub fn remove_session(&mut self, id: &str) -> Option<AsyncSession> {
        eprintln!("SESSION: Removing session {}", id);
        self.sessions.remove(id)
    }
    
    /// Clean up stale sessions
    pub fn cleanup_stale(&mut self) -> Vec<SessionId> {
        let now = Instant::now();
        let timeout = self.timeout;
        
        let stale_ids: Vec<SessionId> = self.sessions
            .iter()
            .filter(|(_, session)| {
                !session.connected && now.duration_since(session.last_active) > timeout
            })
            .map(|(id, _)| id.clone())
            .collect();
        
        for id in &stale_ids {
            eprintln!("SESSION: Cleaning up stale session {}", id);
            self.sessions.remove(id);
        }
        
        stale_ids
    }
    
    /// Get session count
    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }
}

impl Default for AsyncSessionManager {
    fn default() -> Self {
        Self::new()
    }
}
