# nvim-web

Run Neovim in your browser. A WebSocket-based bridge that connects a native Neovim instance to a browser-based WASM UI.

## Overview

nvim-web enables full Neovim functionality in any modern browser by:

- Running Neovim natively on the server via `--embed` mode
- Bridging Neovim's RPC protocol over WebSocket
- Rendering the UI in the browser using Rust/WASM and Canvas

## Architecture

```mermaid
graph TB
    subgraph Browser["Browser (Client)"]
        UI["nvim-web-ui (WASM)"]
        Canvas["HTML5 Canvas"]
        UI --> Canvas
    end
    
    subgraph Host["Host (Server)"]
        WS["WebSocket Server"]
        Bridge["RPC Bridge"]
        Nvim["Neovim --embed"]
        VFS["Virtual FS"]
        
        WS <--> Bridge
        Bridge <--> Nvim
        Bridge <--> VFS
    end
    
    UI <--"WebSocket"--> WS
```

### Data Flow

```mermaid
sequenceDiagram
    participant B as Browser
    participant H as Host
    participant N as Neovim

    B->>H: Connect (WebSocket)
    H->>N: nvim_ui_attach
    N-->>H: Redraw events
    H-->>B: Forward (msgpack)
    B->>B: Render on Canvas
    
    B->>H: Key input
    H->>N: nvim_input
    N-->>H: Updated UI
    H-->>B: Redraw
```

## Components

| Component | Path | Description |
|-----------|------|-------------|
| **nvim-web-host** | `host/` | Rust binary that spawns Neovim and serves WebSocket |
| **nvim-web-ui** | `ui/` | Rust WASM library that renders Neovim UI in browser |
| **Documentation** | `docs/` | Protocol specs, testing guides, architecture notes |

## Quick Start

### Prerequisites

- Rust (stable)
- Neovim 0.9+
- wasm-pack

### Build

```bash
# Build the host
cd host
cargo build --release

# Build the WASM UI  
cd ui
wasm-pack build --target web
```

### Run

```bash
# Terminal 1: Start the host
./host/target/release/nvim-web-host

# Terminal 2: Serve the UI
cd ui && python3 -m http.server 8080

# Open browser
open http://localhost:8080
```

## Architecture Perspectives

### User Perspective

```mermaid
flowchart LR
    User((User)) --> Browser
    Browser --> |"http://localhost:8080"| UI
    UI --> |"Keyboard/Mouse"| Neovim
    Neovim --> |"Rendered UI"| UI
```

Users interact with Neovim through their browser. All keyboard shortcuts, visual modes, and plugins work as expected.

### Developer Perspective

```mermaid
graph TB
    subgraph Development
        RustHost["host/src/*.rs"]
        RustUI["ui/src/*.rs"]
        Tests["host/tests/*.rs"]
    end
    
    subgraph Build
        Cargo["cargo build"]
        WasmPack["wasm-pack build"]
    end
    
    subgraph Output
        Binary["nvim-web-host"]
        WASM["pkg/*.wasm + *.js"]
    end
    
    RustHost --> Cargo --> Binary
    RustUI --> WasmPack --> WASM
    Tests --> Cargo
```

### Production Deployment

```mermaid
flowchart TB
    subgraph Cloud["Production Server"]
        NginxProxy["Nginx (reverse proxy)"]
        HostProcess["nvim-web-host"]
        NvimInstance["Neovim"]
        HostProcess --> NvimInstance
    end
    
    subgraph CDN["Static Assets"]
        StaticFiles["index.html + WASM"]
    end
    
    Users((Users)) --> |"HTTPS"| NginxProxy
    NginxProxy --> |"WSS"| HostProcess
    Users --> |"HTTPS"| CDN
```

### Installation Flow

```mermaid
flowchart TD
    Start([Clone Repository]) --> Deps[Install Dependencies]
    Deps --> |"Rust"| RustUp["rustup (stable)"]
    Deps --> |"Neovim"| NvimInstall["brew/apt install neovim"]
    Deps --> |"WASM"| WasmPack["cargo install wasm-pack"]
    
    RustUp --> Build
    NvimInstall --> Build
    WasmPack --> Build
    
    Build --> BuildHost["cargo build -p nvim-web-host"]
    Build --> BuildUI["wasm-pack build ui/"]
    
    BuildHost --> Run([Start Services])
    BuildUI --> Run
```

## Project Structure

```
nvim-web/
  host/                 # Rust WebSocket server + Neovim bridge
    src/
      main.rs           # Entry point
      ws.rs             # WebSocket handling
      rpc.rs            # Neovim RPC protocol
      nvim.rs           # Neovim process management
      vfs/              # Virtual filesystem backends
    tests/              # Integration tests
  ui/                   # Rust WASM client
    src/
      lib.rs            # WASM entry point + event handling
      renderer.rs       # Canvas rendering
      grid.rs           # Grid state management
      highlight.rs      # Syntax highlighting
    index.html          # HTML entry point
  docs/                 # Documentation
  .github/workflows/    # CI configuration
```

## Features

- Full Neovim UI rendering
- Keyboard input with modifiers (Ctrl, Shift, Alt, Cmd)
- Mouse support (click-to-position, scroll)
- Syntax highlighting
- HiDPI/Retina display support
- Auto-reconnection on page refresh
- Virtual filesystem for browser-based file access

## Contributing

1. Fork the repository
2. Create a feature branch
3. Run tests: `cd host && cargo test`
4. Submit a pull request

## License

MIT

## Acknowledgments

- Neovim team for the excellent `--embed` mode and RPC API
- wasm-pack and wasm-bindgen for the Rust-WASM toolchain
