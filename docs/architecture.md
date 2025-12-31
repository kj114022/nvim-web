# nvim-web Architecture

## System Overview

nvim-web is a dual-component system that bridges native Neovim to the browser.

See also:
- [Testing Philosophy](./testing.md) — Our test + verify approach
- [Protocol Specification](./protocol.md) — WebSocket message format

```mermaid
graph TB
    subgraph "Browser Environment"
        WASM["nvim-web-ui<br/>(Rust → WASM)"]
        Canvas["HTML5 Canvas"]
        DOM["DOM Events<br/>(keyboard, mouse)"]
        
        DOM --> WASM
        WASM --> Canvas
    end
    
    subgraph "Host Environment"
        WS["WebSocket Server<br/>(port 9001)"]
        Bridge["RPC Bridge"]
        Nvim["Neovim Process<br/>(--embed)"]
        VFS["Virtual FS<br/>(local, mem, overlay, ssh)"]
        
        WS <--> Bridge
        Bridge <--> Nvim
        Bridge --> VFS
    end
    
    WASM <--"WebSocket<br/>MessagePack"--> WS
```

## Component Details

### Host (nvim-web-host)

```mermaid
graph LR
    subgraph "nvim-web-host"
        main["main.rs"]
        ws["ws.rs"]
        rpc["rpc.rs"]
        nvim["nvim.rs"]
        vfs["vfs/"]
        sharing["sharing.rs"]
        
        main --> nvim
        main --> ws
        ws --> rpc
        ws --> vfs
        ws --> sharing
        rpc --> nvim
    end
```

| Module | Responsibility |
|--------|----------------|
| `main.rs` | Entry point, spawns Neovim and starts WS server |
| `ws.rs` | WebSocket handling, connection lifecycle, message routing |
| `rpc.rs` | Neovim RPC protocol (msgpack encoding/decoding) |
| `nvim.rs` | Neovim process management (`nvim --embed`) |
| `vfs/` | Virtual filesystem backends (local, memory, overlay, SSH) |
| `sharing.rs` | Share link management and workspace snapshots |

### UI (nvim-web-ui)

```mermaid
graph LR
    subgraph "nvim-web-ui"
        lib["lib.rs"]
        renderer["renderer.rs"]
        grid["grid.rs"]
        highlight["highlight.rs"]
        
        lib --> renderer
        lib --> grid
        lib --> highlight
        renderer --> grid
        renderer --> highlight
    end
```

| Module | Responsibility |
|--------|----------------|
| `lib.rs` | WASM entry point, WebSocket, event handlers |
| `renderer.rs` | Canvas 2D rendering, text drawing, cursor |
| `grid.rs` | Grid state (cells, characters, highlights) |
| `highlight.rs` | Syntax highlight attribute storage |

## Message Flow

### Startup Sequence

```mermaid
sequenceDiagram
    participant H as Host
    participant N as Neovim
    participant B as Browser

    H->>N: spawn nvim --embed
    H->>N: nvim_ui_attach(ext_linegrid)
    N-->>H: UI attached
    H->>H: Listen on :9001
    
    B->>H: WS Connect
    H-->>B: Handshake OK
    H->>N: nvim_ui_try_resize
    N-->>H: grid_resize, grid_line, etc.
    H-->>B: Forward redraw events
    B->>B: Render on canvas
```

### Input Handling

```mermaid
sequenceDiagram
    participant B as Browser
    participant H as Host
    participant N as Neovim

    B->>B: keydown event
    B->>B: Convert to Neovim notation
    B->>H: ["input", "<C-s>"]
    H->>N: nvim_input("<C-s>")
    N-->>H: redraw events
    H-->>B: Updated grid
```

## Threading Model

```mermaid
graph TB
    subgraph "Host Process"
        Main["Main Thread<br/>(WS send, Nvim stdin)"]
        Reader["Neovim Reader Thread<br/>(nvim stdout → channel)"]
        WSReader["WS Reader Thread<br/>(browser → channel)"]
        
        Reader --> |"Arc<Mutex<Receiver>>"| Main
        WSReader --> |"mpsc channel"| Main
    end
```

The host uses three threads:

1. **Main Thread**: Sends to WebSocket, writes to Neovim stdin
2. **Neovim Reader**: Reads from Neovim stdout, sends to shared channel
3. **WS Reader**: Reads from WebSocket, sends to channel

The Neovim reader thread persists across browser reconnections (key fix for stability).

## Virtual Filesystem

```mermaid
graph LR
    subgraph "VFS Backends"
        Local["LocalFs<br/>(/tmp/nvim-web)"]
        Browser["BrowserFs<br/>(OPFS)"]
        SSH["SSH Fs<br/>(on-demand)"]
        Memory["MemoryFs<br/>(ephemeral)"]
        Overlay["OverlayFs<br/>(layered)"]
    end
    
    Manager["VfsManager"] --> Local
    Manager --> Browser
    Manager --> SSH
    Manager --> Memory
    Manager --> Overlay
```

URLs:
- `vfs://local/path` - Server filesystem
- `vfs://browser/path` - Browser OPFS storage
- `vfs://ssh/user@host/path` - Remote via SSH

## Reconnection Architecture

Browser refresh triggers reconnection without losing Neovim state:

```mermaid
sequenceDiagram
    participant B1 as Browser (Tab 1)
    participant H as Host
    participant B2 as Browser (Refresh)

    B1->>H: Connected
    H->>H: Bridge running
    
    Note over B1: User refreshes
    B1--xH: Connection closed
    H->>H: Detect disconnect
    H->>H: Exit bridge, drain channel
    
    B2->>H: New connection
    H->>H: Force redraw
    H-->>B2: Full UI state
```

Key: Neovim reader thread persists, channel shared via `Arc<Mutex<>>`.

## Session Management

Sessions persist across browser disconnections via:

1. **URL parameter**: `?session=<id>` - explicit session binding
2. **localStorage**: Automatic session ID storage
3. **Host-side manager**: `AsyncSessionManager` maintains session pool

```mermaid
graph LR
    Browser -->|session ID| SessionManager
    SessionManager -->|lookup| Session
    Session -->|broadcast channel| Neovim
```

## Session Sharing & Persistence

Beyond basic persistence, sessions can be shared or snapshot:

### Share Links
- **ReadOnly**: View-only access to a running session
- **Time-limited**: Auto-expiry (e.g., 1 hour)
- **Use-limited**: Maximum number of concurrent viewers

### Snapshots
- Captures state: CWD, open files, cursor positions
- Independent of original session (cloned state)
- Resumable as a new session

## Native Messaging (Chrome Extension)

For direct browser extension integration:

```mermaid
sequenceDiagram
    participant Ext as Chrome Extension
    participant Host as nvim-web-host
    participant Nvim as Neovim

    Ext->>Host: stdin JSON message
    Host->>Host: Parse JSON envelope
    Host->>Nvim: Forward to Neovim
    Nvim-->>Host: Response
    Host-->>Ext: stdout JSON response
```

Install native messaging manifest:
```bash
./scripts/install-native-manifest.sh
```

## Security Model

- WebSocket bound to `127.0.0.1` only (localhost)
- Origin validation for allowed origins
- VFS path sandboxing (prevents traversal attacks)
- Session tokens for CLI open commands
