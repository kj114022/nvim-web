//! Async session management using nvim-rs with tokio
//!
//! Each session owns a Neovim instance via nvim-rs and broadcasts
//! redraw events to connected WebSocket clients.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use anyhow::Result;
use async_trait::async_trait;
use futures::io::AsyncWrite;
use nvim_rs::{Handler, Neovim, Value};
use std::process::Stdio;
use tokio::net::TcpStream;
use tokio::process::Command;
use tokio::sync::{broadcast, RwLock as TokioRwLock};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use crate::context::ContextManager;
use nvim_web_vfs::VfsManager;

/// Unique session identifier
pub type SessionId = String;

/// Generate a new unique session ID using UUID v4
pub fn generate_session_id() -> SessionId {
    uuid::Uuid::new_v4().to_string()
}

/// The writer type used by nvim-rs with tokio
/// Using Box<dyn> to support both ChildStdin (local) and TcpStream (remote)
pub type NvimWriter = Box<dyn AsyncWrite + Send + Unpin + 'static>;

use std::sync::{Arc, Mutex};

use tokio::sync::oneshot;

/// Map of pending request IDs to their response/completion channels
pub type RequestMap = Arc<Mutex<HashMap<u32, oneshot::Sender<Value>>>>;

fn generate_unique_request_id(
    map: &std::sync::MutexGuard<HashMap<u32, oneshot::Sender<Value>>>,
) -> u32 {
    loop {
        let req_id = rand::random::<u32>();
        if !map.contains_key(&req_id) {
            return req_id;
        }
    }
}

/// Handler for Neovim notifications - forwards redraw events to broadcast channel
#[derive(Clone)]
pub struct RedrawHandler {
    redraw_tx: broadcast::Sender<Vec<u8>>,
    requests: RequestMap,
    #[allow(dead_code)]
    session_id: String,
    vfs_manager: Arc<TokioRwLock<VfsManager>>,
}

impl RedrawHandler {
    pub fn new(
        session_id: String,
        redraw_tx: broadcast::Sender<Vec<u8>>,
        requests: RequestMap,
        vfs_manager: Arc<TokioRwLock<VfsManager>>,
    ) -> Self {
        Self {
            redraw_tx,
            requests,
            session_id,
            vfs_manager,
        }
    }
}

#[async_trait]
impl Handler for RedrawHandler {
    type Writer = NvimWriter;

    async fn handle_request(
        &self,
        name: String,
        args: Vec<Value>,
        _neovim: Neovim<Self::Writer>,
    ) -> Result<Value, Value> {
        if name == "clipboard_read" {
            let req_id = {
                let map = self.requests.lock().unwrap();
                generate_unique_request_id(&map)
            };
            let (tx, rx) = oneshot::channel();
            {
                let mut map = self.requests.lock().unwrap();
                map.insert(req_id, tx);
            }
            let msg = Value::Array(vec![
                Value::Integer(2.into()),
                Value::String("clipboard_read".into()),
                Value::Array(vec![
                    Value::Integer(req_id.into()),
                    Value::String(self.session_id.clone().into()),
                ]),
            ]);
            let mut bytes = Vec::new();
            if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
                let _ = self.redraw_tx.send(bytes);
            } else {
                return Err(Value::String("Failed to encode clipboard request".into()));
            }
            match tokio::time::timeout(Duration::from_secs(5), rx).await {
                Ok(Ok(val)) => return Ok(val),
                Ok(Err(_)) => return Err(Value::String("Clipboard request channel closed".into())),
                Err(_) => {
                    self.requests.lock().unwrap().remove(&req_id);
                    return Err(Value::String("Clipboard request timed out".into()));
                }
            }
        }

        if name == "vfs_read" {
            let path = args
                .first()
                .and_then(|v| v.as_str())
                .ok_or_else(|| Value::String("vfs_read requires path argument".into()))?;
            let vfs = self.vfs_manager.read().await;
            match vfs.read_file(path).await {
                Ok(content) => {
                    let text = String::from_utf8_lossy(&content);
                    let lines: Vec<Value> = text
                        .lines()
                        .map(|l| Value::String(l.to_string().into()))
                        .collect();
                    return Ok(Value::Array(lines));
                }
                Err(e) => {
                    return Err(Value::String(format!("VFS read error: {e}").into()));
                }
            }
        }

        if name == "vfs_write" {
            let path = args
                .first()
                .and_then(|v| v.as_str())
                .ok_or_else(|| Value::String("vfs_write requires path argument".into()))?;
            let lines = args
                .get(1)
                .and_then(|v| v.as_array())
                .ok_or_else(|| Value::String("vfs_write requires lines argument".into()))?;
            let content: String = lines
                .iter()
                .filter_map(|v| v.as_str())
                .collect::<Vec<_>>()
                .join("\n");
            let vfs = self.vfs_manager.read().await;
            match vfs.write_file(path, content.as_bytes()).await {
                Ok(()) => return Ok(Value::Boolean(true)),
                Err(e) => return Err(Value::String(format!("VFS write error: {e}").into())),
            }
        }

        if name == "vfs_delete" {
            let path = args
                .first()
                .and_then(|v| v.as_str())
                .ok_or_else(|| Value::String("vfs_delete requires path argument".into()))?;
            let vfs = self.vfs_manager.read().await;
            let uri = url::Url::parse(path)
                .map_err(|e| Value::String(format!("Invalid URI: {e}").into()))?;
            let scheme = uri.scheme();
            if let Ok(backend) = vfs.get_backend(scheme).await {
                let p = uri.path().to_string();
                match crate::vfs::async_ops::remove_dir_all(backend.as_ref(), &p).await {
                    Ok(_) => return Ok(Value::Boolean(true)),
                    Err(e) => return Err(Value::String(format!("Delete failed: {e}").into())),
                }
            } else {
                return Err(Value::String(format!("Backend not found: {scheme}").into()));
            }
        }

        Err(Value::String(format!("Unknown request: {name}").into()))
    }

    async fn handle_notify(&self, name: String, args: Vec<Value>, _neovim: Neovim<Self::Writer>) {
        if name == "redraw" {
            let msg = Value::Array(vec![
                Value::Integer(2.into()),
                Value::String("redraw".into()),
                Value::Array(args),
            ]);
            let mut bytes = Vec::new();
            if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
                let _ = self.redraw_tx.send(bytes);
            }
        } else if name == "clipboard_write" {
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
            let cwd = args.first().and_then(|v| v.as_str()).unwrap_or("~");
            let file = args.get(1).and_then(|v| v.as_str()).unwrap_or("");
            let backend = args.get(2).and_then(|v| v.as_str()).unwrap_or("local");
            let git_branch = args
                .get(3)
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty());

            let info_map = vec![
                (Value::String("cwd".into()), Value::String(cwd.into())),
                (Value::String("file".into()), Value::String(file.into())),
                (
                    Value::String("backend".into()),
                    Value::String(backend.into()),
                ),
                (
                    Value::String("git_branch".into()),
                    git_branch.map_or(Value::Nil, |b| Value::String(b.into())),
                ),
            ];
            let msg = Value::Array(vec![Value::String("cwd_info".into()), Value::Map(info_map)]);
            let mut bytes = Vec::new();
            if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
                let _ = self.redraw_tx.send(bytes);
            }
        } else if name == "recording_start" {
            let register = args.first().and_then(|v| v.as_str()).unwrap_or("q");
            let msg = Value::Array(vec![
                Value::String("recording_start".into()),
                Value::String(register.into()),
            ]);
            let mut bytes = Vec::new();
            if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
                let _ = self.redraw_tx.send(bytes);
            }
        } else if name == "recording_stop" {
            let msg = Value::Array(vec![Value::String("recording_stop".into())]);
            let mut bytes = Vec::new();
            if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
                let _ = self.redraw_tx.send(bytes);
            }
        } else if name == "nvim_web_vfx" {
            // args: [{'mode': 'pulse'}]
            if let Some(opts) = args.first().and_then(|v| v.as_map()) {
                let mode = opts
                    .iter()
                    .find(|(k, _)| k.as_str() == Some("mode"))
                    .and_then(|(_, v)| v.as_str())
                    .unwrap_or("railgun");

                let msg = Value::Array(vec![
                    Value::Integer(2.into()),
                    Value::String("vfx_change".into()),
                    Value::Array(vec![Value::String(mode.into())]),
                ]);
                let mut bytes = Vec::new();
                if rmpv::encode::write_value(&mut bytes, &msg).is_ok() {
                    let _ = self.redraw_tx.send(bytes);
                }
            }
        }
    }
}

/// A single async Neovim session
pub struct AsyncSession {
    pub id: SessionId,
    pub nvim: Neovim<NvimWriter>,
    pub redraw_tx: broadcast::Sender<Vec<u8>>,
    pub connections: u32,
    pub created_at: Instant,
    pub last_active: Instant,
    pub connected: bool,
    pub requests: RequestMap,
    pub context_manager: Option<crate::context::ContextManager>,
}

async fn exec_viml(nvim: &Neovim<NvimWriter>, script: &str) -> Result<()> {
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
    /// Create a new session with either a spawned process or remote connection
    #[allow(clippy::too_many_lines)]
    pub async fn new(
        vfs_manager: Arc<TokioRwLock<VfsManager>>,
        context: Option<String>,
        remote_address: Option<String>,
        auth_token: Option<String>,
    ) -> Result<Self> {
        let id = generate_session_id();
        let id_for_log = id.clone();
        let (redraw_tx, _) = broadcast::channel::<Vec<u8>>(256);
        let requests = Arc::new(Mutex::new(HashMap::new()));
        let handler =
            RedrawHandler::new(id.clone(), redraw_tx.clone(), requests.clone(), vfs_manager);

        let nvim = if let Some(addr) = remote_addr(remote_address.clone()) {
            eprintln!("SESSION: Connecting to remote Neovim at {addr}...");

            if addr.starts_with("unix://") {
                let path = addr.trim_start_matches("unix://");
                let stream = tokio::net::UnixStream::connect(path).await?;
                let (reader, writer) = stream.into_split();
                let writer: NvimWriter = Box::new(writer.compat_write());
                let reader = reader.compat();
                let (nvim, io_handler) = Neovim::<NvimWriter>::new(reader, writer, handler);
                tokio::spawn(async move {
                    if let Err(e) = io_handler.await {
                        eprintln!("SESSION {id_for_log}: Remote Unix IO handler error: {e:?}");
                    }
                });
                nvim
            } else {
                let mut stream = TcpStream::connect(addr.trim_start_matches("tcp://")).await?;

                // Perform authentication if token provided
                if let Some(token) = &auth_token {
                    eprintln!("SESSION: Authenticating with remote Neovim...");
                    crate::auth::perform_client_handshake(&mut stream, token).await?;
                }

                let (reader, writer) = stream.into_split();
                let writer: NvimWriter = Box::new(writer.compat_write());
                let reader = reader.compat();
                let (nvim, io_handler) = Neovim::<NvimWriter>::new(reader, writer, handler);
                tokio::spawn(async move {
                    if let Err(e) = io_handler.await {
                        eprintln!("SESSION {id_for_log}: Remote TCP IO handler error: {e:?}");
                    }
                });
                nvim
            }
        } else {
            // Spawn local process
            let mut cmd = Command::new("nvim");
            cmd.arg("--embed");
            if let Ok(home) = std::env::var("HOME") {
                cmd.current_dir(&home);
            }
            if let Ok(cwd) = std::env::current_dir() {
                let plugin_path = cwd.join("plugin");
                if plugin_path.exists() {
                    cmd.args([
                        "--cmd",
                        &format!("set runtimepath+={}", plugin_path.to_string_lossy()),
                    ]);
                }
            }
            // Use piped stdin/stdout/stderr
            cmd.stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped());

            let mut child = cmd.spawn()?;
            let stdin = child.stdin.take().expect("Failed to take stdin");
            let stdout = child.stdout.take().expect("Failed to take stdout");
            let stderr = child.stderr.take().expect("Failed to take stderr");

            // Use ext traits for compat
            let writer: NvimWriter = Box::new(stdin.compat_write());
            let reader = stdout.compat();

            // Neovim::new arguments order: (reader, writer, handler)
            let (nvim, io_handler) = Neovim::<NvimWriter>::new(reader, writer, handler);

            let id_for_io = id_for_log.clone();
            tokio::spawn(async move {
                if let Err(e) = io_handler.await {
                    let msg = format!("{e:?}");
                    if msg.contains("UnexpectedEof") || msg.contains("EOF") {
                        eprintln!("SESSION {id_for_io}: Neovim process exited (clean shutdown)");
                    } else {
                        eprintln!("SESSION {id_for_io}: Local IO handler error: {e:?}");
                    }
                }
            });

            // Monitor stderr
            let id_for_stderr = id_for_log.clone();
            tokio::spawn(async move {
                use tokio::io::AsyncBufReadExt;
                let reader = tokio::io::BufReader::new(stderr);
                let mut lines = reader.lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    eprintln!("NVIM_STDERR [{id_for_stderr}]: {line}");
                }
            });
            nvim
        };

        // Attach UI
        let mut opts = nvim_rs::UiAttachOptions::default();
        opts.set_linegrid_external(true);
        opts.set_multigrid_external(true);
        nvim.ui_attach(80, 24, &opts).await?;

        // Initialize features
        let check_git: std::result::Result<String, _> =
            nvim.command_output("silent! command Git").await;
        if check_git.is_err() || check_git.unwrap_or_default().is_empty() {
            let _ = nvim
                .command("command! -nargs=* Git execute '!' . 'git ' . <q-args>")
                .await;
        }

        // VfsStatus
        let vfs_status_cmd = r"
command! VfsStatus call NvimWeb_ShowVfsStatus()
function! NvimWeb_ShowVfsStatus()
  let l:buf = expand('%:p')
  if l:buf =~# '^vfs://browser/'
    echo 'Backend: browser (OPFS)'
  elseif l:buf =~# '^vfs://ssh/'
    echo 'Backend: ssh (remote)'
  else
    echo 'Backend: local'
  endif
  echo 'Path: ' . (l:buf != '' ? l:buf : '[No Name]')
  echo 'CWD: ' . getcwd()
endfunction
";
        let _ = exec_viml(&nvim, vfs_status_cmd).await;

        // Auto CD
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
        let _ = exec_viml(&nvim, auto_cd_git).await;

        // CWD Sync
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
  let l:git_output = system('git -C ' . shellescape(l:cwd) . ' branch --show-current 2>/dev/null')
  if v:shell_error == 0
    let l:git_branch = trim(l:git_output)
  endif
  let l:backend = 'local'
  if l:file =~# '^vfs://browser/'
    let l:backend = 'browser'
  elseif l:file =~# '^vfs://ssh/'
    let l:backend = 'ssh'
  endif
  call rpcnotify(0, 'cwd_changed', l:cwd, l:file, l:backend, l:git_branch)
endfunction
"#;
        if let Ok(()) = exec_viml(&nvim, cwd_sync).await {
            let _ = nvim.command("call NvimWeb_NotifyCwdChanged()").await;
        }

        // Recording
        let recording_sync = r#"
augroup NvimWebRecording
  autocmd!
  autocmd RecordingEnter * call rpcnotify(0, 'recording_start', reg_recording())
  autocmd RecordingLeave * call rpcnotify(0, 'recording_stop')
augroup END
"#;
        let _ = exec_viml(&nvim, recording_sync).await;

        // Context
        let mut context_manager = None;
        if let Some(ctx_url) = context {
            eprintln!("SESSION: Configuring for context {ctx_url}");
            let cm = ContextManager::new();
            let config = cm.get_config(&ctx_url);
            let _ = nvim
                .command(&format!("set filetype={}", config.filetype))
                .await;
            if config.cmdline == "firenvim" {
                let _ = nvim.command("set laststatus=0").await;
                let _ = nvim.command("set showtabline=0").await;
                let _ = nvim.command("set noruler").await;
            }
            context_manager = Some(cm);
        }

        // Clipboard & WebBrowse (Lua Injection)
        // We use Lua for g:clipboard because it supports function callbacks directly in recent Neovim versions.
        let clipboard_lua = r#"
lua << EOF
local function paste()
  return vim.rpcrequest(0, 'clipboard_read')
end
local function copy(lines, regtype)
  vim.rpcnotify(0, 'clipboard_write', lines, regtype)
end

vim.g.clipboard = {
  name = 'nvim-web',
  copy = {
    ['+'] = copy,
    ['*'] = copy,
  },
  paste = {
    ['+'] = paste,
    ['*'] = paste,
  },
  cache_enabled = 1,
}
EOF
command! WebBrowse call rpcnotify(0, 'nvim_web_action', 'browse_files')
"#;
        let _ = exec_viml(&nvim, clipboard_lua).await;

        eprintln!(
            "SESSION: Created new async session {id} (Remote: {})",
            remote_address.is_some()
        );

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
            context_manager,
        })
    }

    pub fn complete_request(&self, req_id: u32, value: Value) {
        let mut map = self.requests.lock().unwrap();
        if let Some(tx) = map.remove(&req_id) {
            let _ = tx.send(value);
        }
    }

    pub fn touch(&mut self) {
        self.last_active = Instant::now();
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Vec<u8>> {
        self.redraw_tx.subscribe()
    }

    pub async fn input(&self, keys: &str) -> Result<()> {
        self.nvim.input(keys).await?;
        Ok(())
    }

    pub async fn resize(&self, width: i64, height: i64) -> Result<()> {
        self.nvim.ui_try_resize(width, height).await?;
        Ok(())
    }

    pub async fn request_redraw(&self) -> Result<()> {
        self.nvim.command("redraw!").await?;
        Ok(())
    }

    pub async fn rpc_call(&self, method: &str, args: Vec<Value>) -> Result<Value> {
        let outer_result = self.nvim.call(method, args).await;
        match outer_result {
            Ok(inner_result) => {
                inner_result.map_err(|err_value| anyhow::anyhow!("Neovim RPC error: {err_value:?}"))
            }
            Err(call_error) => Err(anyhow::anyhow!("RPC call failed: {call_error:?}")),
        }
    }

    /// Gracefully shutdown the session (save buffers and session state)
    pub async fn shutdown(&self) -> Result<()> {
        if self.connected {
            // Write all modified buffers
            let _ = tokio::time::timeout(Duration::from_secs(2), self.nvim.command("wa")).await;

            // Save full session state (cursor, buffers, undo, windows)
            let session_file = self.session_file_path();
            let cmd = format!("mksession! {}", session_file.display());
            let _ = tokio::time::timeout(Duration::from_secs(2), self.nvim.command(&cmd)).await;
            eprintln!(
                "SESSION {}: Saved state to {}",
                self.id,
                session_file.display()
            );
        }
        Ok(())
    }

    /// Get the session file path for this session
    pub fn session_file_path(&self) -> std::path::PathBuf {
        let session_dir = std::env::var("HOME")
            .map(|h| std::path::PathBuf::from(h).join(".cache/nvim-web/sessions"))
            .unwrap_or_else(|_| std::path::PathBuf::from("/tmp/nvim-web-sessions"));

        // Create directory if it doesn't exist
        let _ = std::fs::create_dir_all(&session_dir);

        session_dir.join(format!("{}.vim", self.id))
    }

    /// Restore a previously saved session state
    pub async fn restore_session(&self) -> Result<bool> {
        let session_file = self.session_file_path();
        if session_file.exists() {
            let cmd = format!("source {}", session_file.display());
            match tokio::time::timeout(Duration::from_secs(3), self.nvim.command(&cmd)).await {
                Ok(Ok(())) => {
                    eprintln!(
                        "SESSION {}: Restored state from {}",
                        self.id,
                        session_file.display()
                    );
                    return Ok(true);
                }
                Ok(Err(e)) => {
                    eprintln!("SESSION {}: Failed to restore session: {e:?}", self.id);
                }
                Err(_) => {
                    eprintln!("SESSION {}: Restore timed out", self.id);
                }
            }
        }
        Ok(false)
    }
}

pub struct AsyncSessionManager {
    sessions: HashMap<SessionId, AsyncSession>,
    pub timeout: Duration,
    pub active_ssh: Option<String>,
    vfs_manager: Arc<TokioRwLock<VfsManager>>,
    pub remote_address: Option<String>,
    pub auth_token: Option<String>,
}

impl AsyncSessionManager {
    pub fn new(vfs_manager: Arc<TokioRwLock<VfsManager>>) -> Self {
        Self {
            sessions: HashMap::new(),
            timeout: Duration::from_secs(300),
            active_ssh: None,
            vfs_manager,
            remote_address: None,
            auth_token: None,
        }
    }

    pub fn set_active_ssh(&mut self, uri: Option<String>) {
        self.active_ssh = uri;
    }

    pub fn set_remote_address(&mut self, addr: String) {
        self.remote_address = Some(addr);
    }

    pub fn set_auth_token(&mut self, token: Option<String>) {
        self.auth_token = token;
    }

    /// Gracefully shutdown all sessions (save buffers)
    pub async fn shutdown_all(&self) {
        let count = self.sessions.len();
        if count > 0 {
            eprintln!("SESSION: Auto-saving {count} active sessions...");
            let futures = self.sessions.values().map(|session| session.shutdown());
            futures::future::join_all(futures).await;
        }
    }

    pub async fn create_session(&mut self, context: Option<String>) -> Result<SessionId> {
        let session = AsyncSession::new(
            self.vfs_manager.clone(),
            context,
            self.remote_address.clone(),
            self.auth_token.clone(),
        )
        .await?;
        let id = session.id.clone();
        self.sessions.insert(id.clone(), session);
        Ok(id)
    }

    pub fn get_session_mut(&mut self, id: &str) -> Option<&mut AsyncSession> {
        self.sessions.get_mut(id)
    }

    pub fn get_session(&self, id: &str) -> Option<&AsyncSession> {
        self.sessions.get(id)
    }

    pub fn has_session(&self, id: &str) -> bool {
        self.sessions.contains_key(id)
    }

    pub fn remove_session(&mut self, id: &str) -> Option<AsyncSession> {
        eprintln!("SESSION: Removing session {id}");
        self.sessions.remove(id)
    }

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

    pub fn session_count(&self) -> usize {
        self.sessions.len()
    }

    pub fn list_sessions(&self) -> Vec<SessionInfo> {
        self.sessions
            .values()
            .map(AsyncSession::to_session_info)
            .collect()
    }

    pub fn session_ids(&self) -> Vec<String> {
        self.sessions.keys().cloned().collect()
    }

    pub fn get_share_link(&self, session_id: &str, host: &str) -> Option<String> {
        if self.has_session(session_id) {
            Some(format!("{host}?session={session_id}"))
        } else {
            None
        }
    }
}

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
    pub fn to_value(&self) -> rmpv::Value {
        rmpv::Value::Map(vec![
            (
                rmpv::Value::String("id".into()),
                rmpv::Value::String(self.id.clone().into()),
            ),
            (
                rmpv::Value::String("name".into()),
                self.name
                    .as_ref()
                    .map_or(rmpv::Value::Nil, |n| rmpv::Value::String(n.clone().into())),
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

/// Helper to normalize remote address string
fn remote_addr(addr: Option<String>) -> Option<String> {
    addr.filter(|s| !s.is_empty())
}
