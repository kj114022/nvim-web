# nvim-web Architecture

Complete overview of the nvim-web codebase.

## System Overview

```
┌─────────────────────────────────────────────────────────────────────────┐
│                              Browser Client                              │
├─────────────────────────────────────────────────────────────────────────┤
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐  ┌─────────────┐  │
│  │ WASM Renderer│  │ Input Handler│  │ CRDT Client  │  │ VFS Client  │  │
│  └──────────────┘  └──────────────┘  └──────────────┘  └─────────────┘  │
│                                    │                                      │
│                          WebSocket / WebTransport                        │
└────────────────────────────────────┼────────────────────────────────────┘
                                     │
                                     ▼
┌─────────────────────────────────────────────────────────────────────────┐
│                              nvim-web Host                               │
├─────────────────────────────────────────────────────────────────────────┤
│  Transport │ Session │ Auth │ API │ Collaboration │ VFS │ K8s          │
├─────────────────────────────────────────────────────────────────────────┤
│                         Neovim Process (--embed)                         │
└─────────────────────────────────────────────────────────────────────────┘
```

## Crate Structure

| Crate | Path | Description |
|-------|------|-------------|
| `nvim-web-host` | `crates/host` | Main server binary and library |
| `nvim-web-ui` | `crates/ui` | Browser WASM frontend |
| `nvim-web-vfs` | `crates/vfs` | Virtual filesystem abstraction |
| `nvim-web-protocol` | `crates/protocol` | Shared message types |

## Host Modules

### Core (`crates/host/src/`)

| Module | Description |
|--------|-------------|
| `session.rs` | Neovim process lifecycle and RPC |
| `context.rs` | Shared application state |
| `config.rs` | Configuration loading |
| `api.rs` | REST API routes |
| `main.rs` | Server entry point |
| `embedded.rs` | Embedded static assets |

### WebSocket (`ws/`)

| File | Description |
|------|-------------|
| `mod.rs` | WebSocket server |
| `connection.rs` | Connection handling |
| `protocol.rs` | Message encoding/decoding |
| `commands.rs` | RPC command handlers |
| `rate_limit.rs` | Request rate limiting |

### Transport (`transport/`)

| File | Description |
|------|-------------|
| `mod.rs` | `Transport` trait abstraction |
| `websocket.rs` | WebSocket implementation |
| `webtransport.rs` | QUIC/HTTP3 via wtransport |

### Collaboration (`crdt/`)

| File | Description |
|------|-------------|
| `mod.rs` | CrdtManager - per-session docs |
| `buffer.rs` | BufferCrdt - Y.Doc wrapper |
| `sync.rs` | Y-sync protocol handling |

| File | Description |
|------|-------------|
| `collaboration.rs` | Multi-user sessions, cursor sync |

### Authentication (`oidc/`)

| File | Description |
|------|-------------|
| `mod.rs` | AuthUser, AccessPolicy |
| `config.rs` | AuthConfig with presets |
| `client.rs` | OIDC client with PKCE |
| `routes.rs` | Login/callback/logout routes |
| `middleware.rs` | Session validation |

| File | Description |
|------|-------------|
| `auth.rs` | HMAC-SHA256 challenge-response |

### Kubernetes (`k8s/`)

| File | Description |
|------|-------------|
| `mod.rs` | K8sConfig, PodResources |
| `pod_manager.rs` | Pod lifecycle management |
| `session_pod.rs` | Pod spec builder |
| `router.rs` | Session routing types |

### Additional Modules

| File | Description |
|------|-------------|
| `vfs_handlers.rs` | VFS RPC command handlers |
| `terminal.rs` | PTY management |
| `search.rs` | Host-side ripgrep search |
| `trace.rs` | Latency tracing |
| `tunnel.rs` | SSH tunnel management |
| `sharing.rs` | Session sharing |
| `settings.rs` | User settings |
| `project.rs` | Project configuration |
| `git.rs` | Git operations |
| `native.rs` | Native UI launch |

### Universal Pipe (`pipe.rs`)

| Feature | Description |
|---------|-------------|
| `run_pipe()` | Execute CLI with stdin/stdout |
| `run_pipe_streaming()` | Stream output chunks |
| `validate_tool()` | Check if command exists |

### Backend Swap (`backend_swap.rs`)

| Type | Description |
|------|-------------|
| `BackendType` | Local, Docker, SSH, TCP |
| `VfsBackend` | Local, Git, GitHub, Browser, SFTP |
| `BackendSwap` | Hot-swap Neovim backends |
| `VfsSwap` | Hot-swap filesystems |

## UI Modules (`crates/ui/src/`)

| File | Description |
|------|-------------|
| `lib.rs` | WASM entry point |
| `worker.rs` | Web Worker handling |
| `renderer.rs` | Canvas 2D rendering |
| `grid.rs` | Neovim grid state |
| `input.rs` | Keyboard/mouse input |
| `input_queue.rs` | Input event queuing |
| `dom.rs` | DOM manipulation |
| `fs/` | Browser filesystem (OPFS) |

### TypeScript (`ts/`)

| File | Description |
|------|-------------|
| `p2p.ts` | WebRTC P2P mesh |
| `chat.ts` | Chat panel UI |
| `prediction.ts` | Client-side Lua prediction |
| `session_storage.ts` | Session persistence |

## VFS Modules (`crates/vfs/src/`)

| File | Description |
|------|-------------|
| `lib.rs` | VfsManager and traits |
| `local.rs` | Local filesystem |
| `browser.rs` | OPFS browser storage |
| `ssh.rs` | SFTP via russh |
| `github.rs` | GitHub API |
| `overlay.rs` | Layered filesystem |
| `memory.rs` | In-memory filesystem |

## Protocol (`crates/protocol/src/`)

| File | Description |
|------|-------------|
| `lib.rs` | Module exports |
| `messages.rs` | Message types |
| `rpc.rs` | RPC utilities |

## Key Data Flows

### Connection

```
Browser → HTTP GET / → Host serves index.html + WASM
Browser → WS/WebTransport connect → Host upgrades
Browser → create_session → Host spawns nvim --embed
```

### Rendering

```
Neovim → redraw events → Host → render_batch → Browser → Canvas
```

### CRDT Sync

```
Client A → SyncStep1 → Host
Host → SyncStep2 → Client A (missing updates)
Client A → Update → Host → broadcast → Client B
```

## Configuration Files

| File | Purpose |
|------|---------|
| `config.toml` | Server configuration |
| `config.example.toml` | Example with all options |
| `config.js` | Browser-side configuration |

## Test Files

| Directory | Description |
|-----------|-------------|
| `crates/host/tests/` | Integration tests |
| `e2e/` | Playwright browser tests |
