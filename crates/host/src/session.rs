//! Async session management using nvim-rs with tokio
//!
//! Each session owns a Neovim instance via nvim-rs and broadcasts
//! redraw events to connected WebSocket clients.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;
use nvim_rs::compat::tokio::Compat;
use nvim_rs::create::tokio::new_child_cmd;
use nvim_rs::{Handler, Neovim, Value};
use tokio::process::{ChildStdin, Command};
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
    format!("{now:x}")
}

/// The writer type used by nvim-rs with tokio
pub type NvimWriter = Compat<ChildStdin>;

use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

/// Map of pending request IDs to their response/completion channels
pub type RequestMap = Arc<Mutex<HashMap<u32, oneshot::Sender<Value>>>>;

/// Handler for Neovim notifications - forwards redraw events to broadcast channel
#[derive(Clone)]
pub struct RedrawHandler {
    redraw_tx: broadcast::Sender<Vec<u8>>,
    requests: RequestMap,
    #[allow(dead_code)] // Kept for debugging purposes
    session_id: String,
}

impl RedrawHandler {
    pub const fn new(
        session_id: String,
        redraw_tx: broadcast::Sender<Vec<u8>>,
        requests: RequestMap,
    ) -> Self {
        Self {
            redraw_tx,
            requests,
            session_id,
        }
    }
}

#[async_trait]
impl Handler for RedrawHandler {
    type Writer = NvimWriter;

    async fn handle_request(
        &self,
        name: String,
        _args: Vec<Value>,
        _neovim: Neovim<Self::Writer>,
    ) -> Result<Value, Value> {
        if name == "clipboard_read" {
            // Args: [regtype] (unneeded for browser usually, but good to have)
            // Generate request ID
            let req_id = rand::random::<u32>(); // Simple random ID

            // Create channel
            let (tx, rx) = oneshot::channel();

            // Store sender
            {
                let mut map = self.requests.lock().unwrap();
                map.insert(req_id, tx);
            }

            // Send request to browser
            // Format: [2, "clipboard_read", [req_id]]
            // Using Type 2 (Notification) mechanism with specific method
            let msg = Value::Array(vec![
                Value::Integer(2.into()),
                Value::String("clipboard_read".into()),
                Value::Array(vec![Value::Integer(req_id.into())]),
            ]);

            let mut bytes = Vec::new();
            if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
                let _ = self.redraw_tx.send(bytes);
            } else {
                return Err(Value::String("Failed to encode clipboard request".into()));
            }

            // Wait for response with timeout
            match tokio::time::timeout(Duration::from_secs(5), rx).await {
                Ok(Ok(val)) => return Ok(val),
                Ok(Err(_)) => return Err(Value::String("Clipboard request channel closed".into())),
                Err(_) => {
                    // Timeout - ensure we remove the sender
                    self.requests.lock().unwrap().remove(&req_id);
                    return Err(Value::String("Clipboard request timed out".into()));
                }
            }
        }

        Err(Value::String(format!("Unknown request: {name}").into()))
    }

    async fn handle_notify(&self, name: String, args: Vec<Value>, _neovim: Neovim<Self::Writer>) {
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
        } else if name == "clipboard_write" {
            // Args: [lines (Array), regtype (String)]
            // Send to browser: [2, "clipboard_write", [lines, regtype]]
            let msg = Value::Array(vec![
                Value::Integer(2.into()),
                Value::String("clipboard_write".into()),
                Value::Array(args),
            ]);

            let mut bytes = Vec::new();
            if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
                let _ = self.redraw_tx.send(bytes);
            }
        } else if name == "cwd_changed" {
            // Real-time CWD sync: [cwd, file, backend, git_branch]
            let cwd = args.first().and_then(|v| v.as_str()).unwrap_or("~");
            let file = args.get(1).and_then(|v| v.as_str()).unwrap_or("");
            let backend = args.get(2).and_then(|v| v.as_str()).unwrap_or("local");
            let git_branch = args
                .get(3)
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty());

            // Build cwd_info message for UI
            // Format: ["cwd_info", {cwd, file, backend, git_branch}]
            let info_map = vec![
                (Value::String("cwd".into()), Value::String(cwd.into())),
                (Value::String("file".into()), Value::String(file.into())),
                (
                    Value::String("backend".into()),
                    Value::String(backend.into()),
                ),
                (
                    Value::String("git_branch".into()),
                    git_branch
                        .map_or(Value::Nil, |b| Value::String(b.into())),
                ),
            ];

            let msg = Value::Array(vec![Value::String("cwd_info".into()), Value::Map(info_map)]);

            let mut bytes = Vec::new();
            if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
                let _ = self.redraw_tx.send(bytes);
                eprintln!("SESSION: CWD changed -> {cwd} (git: {git_branch:?})");
            }
        }
    }
}

/// A single async Neovim session
pub struct AsyncSession {
    pub id: SessionId,
    pub nvim: Neovim<NvimWriter>,
    pub redraw_tx: broadcast::Sender<Vec<u8>>,
    pub connections: u32, // Track active WebSocket connections
    pub created_at: Instant,
    pub last_active: Instant,
    pub connected: bool,
    pub requests: RequestMap,
}

// Helper: Execute multi-line VimL via nvim_exec2
async fn exec_viml(nvim: &Neovim<NvimWriter>, script: &str) -> Result<()> {
    // nvim_exec2(src, opts) - opts is a map with "output" key
    let opts = vec![(Value::String("output".into()), Value::Boolean(false))];
    let _ = nvim
        .call(
            "nvim_exec2",
            vec![Value::String(script.into()), Value::Map(opts)],
        )
        .await?;
    Ok(())
}

impl AsyncSession {
    /// Create a new session with a freshly spawned Neovim instance
    #[allow(clippy::too_many_lines)]
    pub async fn new() -> Result<Self> {
        let id = generate_session_id();
        let id_for_log = id.clone();

        // Create broadcast channel for redraw events
        let (redraw_tx, _) = broadcast::channel::<Vec<u8>>(256);

        // Create shared request map for host<->browser RPC
        let requests = Arc::new(Mutex::new(HashMap::new()));

        // Create handler that forwards redraws
        let handler = RedrawHandler::new(id.clone(), redraw_tx.clone(), requests.clone());

        // Spawn neovim using nvim-rs
        // --embed: Use stdin/stdout for msgpack RPC
        // User's init.lua will be loaded (includes plugins like vim-fugitive)
        let mut cmd = Command::new("nvim");
        cmd.arg("--embed");

        // Set working directory to user's home or current dir
        if let Ok(home) = std::env::var("HOME") {
            cmd.current_dir(&home);
        }

        // Add plugin to runtime path (development mode)
        // Checks if we are running from project root and adds 'plugin' dir
        if let Ok(cwd) = std::env::current_dir() {
            let plugin_path = cwd.join("plugin");
            if plugin_path.exists() {
                eprintln!("SESSION: Adding plugin path: {}", plugin_path.display());
                cmd.args([
                    "--cmd",
                    &format!("set runtimepath+={}", plugin_path.to_string_lossy()),
                ]);
            }
        }

        let (nvim, io_handler, _child) = new_child_cmd(&mut cmd, handler).await?;

        // Spawn the IO handler task
        tokio::spawn(async move {
            if let Err(e) = io_handler.await {
                eprintln!("SESSION {id_for_log}: IO handler error: {e:?}");
            }
        });

        // Attach UI with ext_linegrid for modern grid-based rendering
        let mut opts = nvim_rs::UiAttachOptions::default();
        opts.set_linegrid_external(true);
        nvim.ui_attach(80, 24, &opts).await?;

        // Set up minimal Git command if fugitive isn't available
        // Using :command instead of Lua for reliability
        let check_git = nvim.command_output("silent! command Git").await;
        if check_git.is_err() || check_git.unwrap_or_default().is_empty() {
            let git_cmd = "command! -nargs=* Git execute '!' . 'git ' . <q-args>";
            if let Err(e) = nvim.command(git_cmd).await {
                eprintln!("SESSION: Git command setup failed: {e:?}");
            } else {
                eprintln!("SESSION: Git command wrapper registered");
            }
        } else {
            eprintln!("SESSION: Git command already exists (likely fugitive)");
        }

        // Set up VfsStatus command to show current backend/path info
        let vfs_status_cmd = r"
command! VfsStatus call NvimWeb_ShowVfsStatus()

function! NvimWeb_ShowVfsStatus()
  let l:buf = expand('%:p')
  if l:buf =~# '^vfs://browser/'
    echo 'Backend: browser (OPFS)'
  elseif l:buf =~# '^vfs://ssh/'
    echo 'Backend: ssh (remote)'
  else
    echo 'Backend: local (server filesystem)'
  endif
  echo 'Path: ' . (l:buf != '' ? l:buf : '[No Name]')
  echo 'CWD: ' . getcwd()
endfunction
";
        if let Err(e) = exec_viml(&nvim, vfs_status_cmd).await {
            eprintln!("SESSION: VfsStatus command setup failed: {e:?}");
        } else {
            eprintln!("SESSION: VfsStatus command registered");
        }

        // Set up auto-CD to git root when opening files
        let auto_cd_git = r"
augroup NvimWebGitCD
  autocmd!
  autocmd BufEnter * call NvimWeb_AutoCdToGitRoot()
augroup END

function! NvimWeb_AutoCdToGitRoot()
  let l:file = expand('%:p')
  if l:file == '' || l:file =~# '^term://' || l:file =~# '^fugitive://'
    return
  endif
  let l:git_root = system('git -C ' . shellescape(expand('%:p:h')) . ' rev-parse --show-toplevel 2>/dev/null')
  if v:shell_error == 0 && l:git_root != ''
    execute 'lcd ' . fnameescape(trim(l:git_root))
  endif
endfunction
";
        if let Err(e) = exec_viml(&nvim, auto_cd_git).await {
            eprintln!("SESSION: Auto-CD setup failed: {e:?}");
        } else {
            eprintln!("SESSION: Auto-CD to git root enabled");
        }

        // Set up real-time CWD sync - notifies host on DirChanged and BufEnter
        let cwd_sync = r#"
augroup NvimWebCwdSync
  autocmd!
  autocmd DirChanged * call NvimWeb_NotifyCwdChanged()
  autocmd BufEnter * call NvimWeb_NotifyCwdChanged()
augroup END

function! NvimWeb_NotifyCwdChanged()
  let l:cwd = getcwd()
  let l:file = expand('%:p')
  let l:git_branch = ''
  
  " Get git branch if in a git repo
  let l:git_output = system('git -C ' . shellescape(l:cwd) . ' branch --show-current 2>/dev/null')
  if v:shell_error == 0
    let l:git_branch = trim(l:git_output)
  endif
  
  " Determine backend from file path
  let l:backend = 'local'
  if l:file =~# '^vfs://browser/'
    let l:backend = 'browser'
  elseif l:file =~# '^vfs://ssh/'
    let l:backend = 'ssh'
  endif
  
  " Notify the host
  call rpcnotify(0, 'cwd_changed', l:cwd, l:file, l:backend, l:git_branch)
endfunction
"#;
        if let Err(e) = exec_viml(&nvim, cwd_sync).await {
            eprintln!("SESSION: CWD sync setup failed: {e:?}");
        } else {
            eprintln!("SESSION: Real-time CWD sync enabled");
            // Trigger initial sync
            if let Err(e) = nvim.command("call NvimWeb_NotifyCwdChanged()").await {
                eprintln!("SESSION: Initial CWD sync failed: {e:?}");
            }
        }

        eprintln!("SESSION: Created new async session {id}");

        let now = Instant::now();
        Ok(Self {
            id,
            nvim,
            redraw_tx,
            created_at: now,
            last_active: now,
            connected: false,
            connections: 0,
            requests,
        })
    }

    /// Complete a pending request (e.g. from clipboard read)
    pub fn complete_request(&self, req_id: u32, value: Value) {
        let mut map = self.requests.lock().unwrap();
        if let Some(tx) = map.remove(&req_id) {
            let _ = tx.send(value);
        }
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

    /// Execute RPC call to Neovim and return result
    pub async fn rpc_call(&self, method: &str, args: Vec<Value>) -> Result<Value> {
        let outer_result = self.nvim.call(method, args).await;

        match outer_result {
            Ok(inner_result) => {
                inner_result
                    .map_err(|err_value| anyhow::anyhow!("Neovim RPC error: {err_value:?}"))
            }
            Err(call_error) => Err(anyhow::anyhow!("RPC call failed: {call_error:?}")),
        }
    }
}

/// Async manager for multiple Neovim sessions
pub struct AsyncSessionManager {
    sessions: HashMap<SessionId, AsyncSession>,
    pub timeout: Duration,
    /// Active SSH connection URI (if any)
    pub active_ssh: Option<String>,
}

impl AsyncSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: HashMap::new(),
            timeout: Duration::from_secs(300), // 5 minutes
            active_ssh: None,
        }
    }

    /// Set active SSH connection URI
    pub fn set_active_ssh(&mut self, uri: Option<String>) {
        self.active_ssh = uri;
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
        eprintln!("SESSION: Removing session {id}");
        self.sessions.remove(id)
    }

    /// Clean up stale sessions
    pub fn cleanup_stale(&mut self) -> Vec<SessionId> {
        let now = Instant::now();
        let timeout = self.timeout;

        let stale_ids: Vec<SessionId> = self
            .sessions
            .iter()
            .filter(|(_, session)| {
                !session.connected && now.duration_since(session.last_active) > timeout
            })
            .map(|(id, _)| id.clone())
            .collect();

        for id in &stale_ids {
            eprintln!("SESSION: Cleaning up stale session {id}");
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

/// Session metadata for API responses
#[derive(Debug, Clone, serde::Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub name: Option<String>,
    pub created_at_secs: u64,
    pub age_secs: u64,
    pub connected: bool,
    pub is_active: bool,
}

impl SessionInfo {
    /// Convert to `MessagePack` Value for RPC
    pub fn to_value(&self) -> rmpv::Value {
        rmpv::Value::Map(vec![
            (
                rmpv::Value::String("id".into()),
                rmpv::Value::String(self.id.clone().into()),
            ),
            (
                rmpv::Value::String("name".into()),
                self.name.as_ref().map_or(rmpv::Value::Nil, |n| rmpv::Value::String(n.clone().into()))
            ),
            (
                rmpv::Value::String("created_at_secs".into()),
                rmpv::Value::Integer(self.created_at_secs.into()),
            ),
            (
                rmpv::Value::String("age_secs".into()),
                rmpv::Value::Integer(self.age_secs.into()),
            ),
            (
                rmpv::Value::String("connected".into()),
                rmpv::Value::Boolean(self.connected),
            ),
            (
                rmpv::Value::String("is_active".into()),
                rmpv::Value::Boolean(self.is_active),
            ),
        ])
    }
}

impl AsyncSession {
    /// Get session info for API
    pub fn to_session_info(&self) -> SessionInfo {
        let now = Instant::now();
        SessionInfo {
            id: self.id.clone(),
            name: None,
            created_at_secs: self.created_at.elapsed().as_secs(),
            age_secs: now.duration_since(self.created_at).as_secs(),
            connected: self.connected,
            is_active: self.redraw_tx.receiver_count() > 0,
        }
    }
}

impl AsyncSessionManager {
    /// List all sessions with metadata
    pub fn list_sessions(&self) -> Vec<SessionInfo> {
        self.sessions
            .values()
            .map(AsyncSession::to_session_info)
            .collect()
    }

    /// Get session IDs
    pub fn session_ids(&self) -> Vec<String> {
        self.sessions.keys().cloned().collect()
    }

    /// Generate a shareable link for a session
    pub fn get_share_link(&self, session_id: &str, host: &str) -> Option<String> {
        if self.has_session(session_id) {
            Some(format!("{host}?session={session_id}"))
        } else {
            None
        }
    }
}
