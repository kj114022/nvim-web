# nvim-web

Neovim in the browser. Real Neovim, not an emulation.

## Philosophy

This project transports Neovim to the browser—it does not reimplement it. The full power of Neovim (plugins, LSP, treesitter, your config) works natively because we embed the real thing.

**Design principles:**

- **Transport, don't reimplement** — The protocol carries Neovim events faithfully
- **Test + Verify** — Type-safe contracts via Rust; empirical testing for real-world failures
- **Minimal by default** — No bloat; features behind settings

## Quick Start

```bash
# Build and start (single binary - all assets embedded)
cargo build --release -p nvim-web-host
./target/release/nvim-web-host

# Open browser
open http://localhost:8080
```

## Magic Link

Open any project from terminal directly in the browser:

```bash
nvim-web open /path/to/project
nvim-web open .
```

## Features

- Full Neovim rendering on canvas
- Single binary (UI assets embedded)
- Session persistence across refreshes
- VFS backends: local, browser OPFS, SSH
- Real-time CWD and git branch sync
- PWA installable

## CLI

```bash
nvim-web                  # Start server
nvim-web open [PATH]      # Open project in browser
nvim-web --help           # Help
nvim-web --version        # Version
```

## Keyboard

All Neovim keybindings work. The UI intercepts browser shortcuts (Cmd+W, etc.) to prevent conflicts.

## VFS Commands

| Command | Description |
| ------- | ----------- |
| `:e path` | Open file |
| `:E @local/path` | Server filesystem |
| `:E @browser/path` | Browser OPFS |
| `:E @ssh/user@host/path` | SSH remote |
| `:VfsStatus` | Current backend |

## URL Parameters

| Parameter | Description |
| --------- | ----------- |
| `?session=new` | Force new session |
| `?session=<id>` | Reconnect to session |
| `?open=<token>` | Magic link token |

## Architecture

```text
Browser (WASM)          Host (Rust)           Neovim
+-----------+          +------------+        +-------+
| Canvas UI | <--WS--> | nvim-web   | <--->  | nvim  |
+-----------+          | -host      |        +-------+
                       +------------+
```

See [docs/architecture.md](./docs/architecture.md) for details.

## Testing

We apply Verification-Guided Development:

- **Verified**: Trait contracts (`VfsBackend`)
- **Tested**: Differential tests across backends
- **Empirical**: Real-world failure modes (network, browser quirks)

See [docs/testing.md](./docs/testing.md) for our testing philosophy.

## Project Structure

| Directory | Purpose |
| --------- | ------- |
| `crates/host` | Rust host with embedded UI |
| `crates/ui` | WASM browser UI |
| `crates/vfs` | Virtual filesystem backends |
| `crates/protocol` | Message types |
| `plugin/` | Neovim VFS plugin |
| `docs/` | Architecture, protocol, testing docs |

## Requirements

- Rust 1.70+
- Neovim 0.9+
- wasm-pack (for UI development only)

## License

MIT
