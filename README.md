# nvim-web

Neovim in the browser. Real Neovim, not an emulation.

## Quick Start

```bash
# Build and start
cargo run --release -p nvim-web-host

# Open browser
open http://localhost:8080

# Or run with Docker (includes SSH test server)
docker-compose up

```

## Features

- **Real Neovim** - Your config, plugins, LSP, treesitter all work
- **Single Binary** - UI assets embedded, no external dependencies
- **Multi-grid Support** - Split windows, floating windows, cmdline
- **VFS Backends** - Local, Overlay, Memory, Browser OPFS, SSH/SFTP, Git, HTTP
- **Session Sharing** - Share read-only links and create workspace snapshots
- **Session Persistence** - Reconnect to existing sessions
- **PWA Installable** - Install as desktop app
- **Keyboard Passthrough** - All Neovim keybindings work

## CLI

```bash
nvim-web                  # Start server on localhost:8080
nvim-web open [PATH]      # Open project in browser
nvim-web --help           # Help
nvim-web --version        # Version
```

## Magic Link

Open projects instantly from terminal with optional QR codes and GitHub support.

### Local Projects

```bash
# Open current directory
nvim-web open .

# Open file at specific line
nvim-web open src/main.rs:42

# With QR code for mobile
nvim-web open . --qr

# Shareable link (1 hour expiry)
nvim-web open . --share --duration 1h
```

### GitHub Repositories

```bash
# Clone and open any GitHub repo
nvim-web open github.com/neovim/neovim

# Open specific file at line
nvim-web open github.com/owner/repo/blob/main/src/lib.rs#L42
```

### Options

| Option | Description |
|--------|-------------|
| `--qr` | Display scannable QR code |
| `--share` | Create multi-use link |
| `--duration` | Link expiry (e.g., `1h`, `30m`, `1d`) |
| `-f, --file` | Target file to open |
| `-l, --line` | Line number to jump to |

## Install

```bash
# From source
./install.sh

# Or manually
cargo build --release -p nvim-web-host
cp target/release/nvim-web-host /usr/local/bin/nvim-web
```

## Configuration

Copy `config.example.toml` to `~/.config/nvim-web/config.toml`:

```toml
[server]
ws_port = 9001
http_port = 8080
bind = "127.0.0.1"

[session]
timeout = 300
max_sessions = 10
```

## VFS Backends

| Backend | URI Format | Description |
|---------|------------|-------------|
| Local | `/path/to/file` | Server filesystem |
| SSH | `vfs://ssh/user@host:22/path` | SFTP access |
| Git | `vfs://git/.@HEAD/path` | Git history |
| HTTP | `vfs://http/https://...` | Remote files (read-only) |
| Browser | `vfs://browser/path` | Browser OPFS storage |
| Overlay | (Configuration only) | Layered filesystem (Read-only + Write layer) |
| Memory | (Configuration only) | In-memory ephemeral storage |

## Architecture

```text
Browser (WASM)          Host (Rust)           Neovim
+-----------+          +------------+        +-------+
| Canvas UI | <--WS--> | nvim-web   | <--->  | nvim  |
+-----------+          | host       |        +-------+
                       +------------+
```

### Crates

| Crate | Description |
|-------|-------------|
| `nvim-web-host` | HTTP/WebSocket server with embedded UI |
| `nvim-web-ui` | Browser UI (WASM, canvas rendering) |
| `nvim-web-vfs` | Virtual filesystem backends |
| `nvim-web-protocol` | Shared message types |

## Development

```bash
# Build all crates
cargo build

# Run tests
cargo test

# Build UI (WASM) - only needed when modifying UI
cd crates/ui && wasm-pack build --target web

# Build release
cargo build --release -p nvim-web-host

# Format and lint
cargo fmt && cargo clippy
```

## Requirements

- Rust 1.70+
- Neovim 0.9+
- wasm-pack (UI development only)

## License

MIT
