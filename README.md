# nvim-web

Neovim in the browser. Runs the actual Neovim binary on a host machine and renders via WebAssembly over WebSocket/WebTransport. Your config, plugins, LSP, and Treesitter work exactly as in a native session.

## Features

| Feature | Description |
|---------|-------------|
| **Full Neovim** | Your config, plugins, LSP, Treesitter all work |
| **Real-time Collaboration** | CRDT-based editing via y-crdt |
| **WebTransport** | QUIC/HTTP3 for sub-50ms latency |
| **Enterprise SSO** | OIDC with Google, Okta, Azure AD |
| **Kubernetes** | Pod-per-session horizontal scaling |
| **Virtual Filesystems** | Local, SSH, GitHub, Browser, Git, SFTP |
| **Terminal PTY** | Full terminal emulator via xterm.js |
| **Universal Tool Pipe** | Any CLI tool (claude, gemini, prettier) |
| **Hot Backend Swap** | Docker/SSH/TCP without losing state |
| **P2P Chat** | Browser-to-browser encrypted messaging |
| **Bazel Build** | Hermetic builds with rules_rust |

## Quick Start

```bash
cargo install --git https://github.com/kj114022/nvim-web nvim-web-host
nvim-web
```

Open `http://localhost:8080` in your browser.

## Installation

### From Source

```bash
git clone https://github.com/kj114022/nvim-web && cd nvim-web
cargo build --release -p nvim-web-host
```

### Bazel Build

```bash
CARGO_BAZEL_REPIN=1 bazel sync --only=crate_index
bazel build //...
```

### Package Managers

| Platform | Command |
|----------|---------|
| **macOS** | `brew install --build-from-source packaging/nvim-web.rb` |
| **Ubuntu/Debian** | `sudo dpkg -i nvim-web_0.1.0_amd64.deb` |
| **Ubuntu Snap** | `sudo snap install nvim-web` |
| **Fedora/RHEL** | `sudo dnf install nvim-web-0.1.0-1.x86_64.rpm` |
| **Arch Linux** | `cd packaging/arch && makepkg -si` |
| **NixOS** | `nix build github:kj114022/nvim-web#nvim-web` |
| **Flatpak** | `flatpak install com.github.kj114022.nvim-web` |
| **Docker** | `docker run -p 8080:8080 ghcr.io/kj114022/nvim-web` |

## Usage

```bash
nvim-web                              # Start server on :8080
nvim-web --port 3000 --bind 0.0.0.0   # Custom port, network access
nvim-web open /path/to/project        # Open project
nvim-web open github.com/user/repo    # Clone and open GitHub repo
```

### Universal Tool Pipe

```lua
-- Execute any CLI tool from Neovim
:ToolExec claude -p "explain this code"
:ToolExec prettier --stdin-filepath %

-- Lua API
local pipe = require("nvim-web.pipe")
pipe.exec("gemini", {"--prompt", "fix"}, selection)
```

### Backend Swap

Seamlessly switch backends without losing state:

| Backend | URL Format |
|---------|------------|
| Local | `local` |
| Docker | `docker:container-name` |
| SSH | `ssh://user@host:port` |
| TCP | `tcp://host:port` |

### VFS Swap

Switch filesystems on the fly:

| VFS | URL Format |
|-----|------------|
| Local | `local:/path/to/dir` |
| Git | `git:https://github.com/user/repo.git@branch` |
| GitHub | `github:owner/repo@ref` |
| Browser | `browser:session-id` |
| SFTP | `sftp://user@host:/path` |

## Architecture

```
Browser (WASM)              nvim-web Host                Neovim
┌──────────────┐           ┌─────────────────┐          ┌──────────┐
│ Renderer     │◄─────────►│ WebSocket/QUIC  │◄────────►│ --embed  │
│ Input        │  msgpack  │ VFS / CRDT      │  RPC     │ process  │
│ P2P Chat     │           │ Pipe / Swap     │          │          │
└──────────────┘           └─────────────────┘          └──────────┘
```

## Documentation

| Document | Description |
|----------|-------------|
| [architecture.md](docs/architecture.md) | Codebase structure |
| [webtransport.md](docs/webtransport.md) | QUIC configuration |
| [collaboration.md](docs/collaboration.md) | Real-time editing |
| [authentication.md](docs/authentication.md) | OIDC setup |
| [kubernetes.md](docs/kubernetes.md) | K8s deployment |

## Project Structure

```
nvim-web/
├── crates/
│   ├── host/           # Server: transport, pipe, swap, terminal
│   ├── ui/             # WASM: renderer, p2p chat, prediction
│   ├── vfs/            # VFS: local, ssh, github, git
│   └── protocol/       # Shared message types
├── WORKSPACE           # Bazel workspace
├── k8s/                # Kubernetes manifests
└── packaging/          # deb, rpm, arch, nix, snap, flatpak
```

## Development

```bash
cargo build                              # Build all
cargo test                               # Run tests (76 tests)
RUST_LOG=debug cargo run -p nvim-web-host  # Run with logging
cargo fmt && cargo clippy                # Format and lint
```

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md).

## License

MIT