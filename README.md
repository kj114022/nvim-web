# nvim-web

Neovim in the Browser - Full Neovim experience with WebSocket bridge.

## Quick Start

```bash
# Start host
cd host && cargo run --release

# Serve UI (in another terminal)
cd ui && python3 -m http.server 8080

# Open browser
open http://localhost:8080
```

## Features

- Full Neovim rendering in browser
- Keyboard and mouse support
- Clipboard paste integration
- Multi-session support
- VFS with local, SSH, and browser storage
- PWA installable
- Neovim plugin for file explorer

## Components

| Directory | Description |
|-----------|-------------|
| `host/` | Rust WebSocket host (nvim-web-host) |
| `ui/` | WASM browser UI |
| `plugin/` | Neovim Lua plugin |
| `extension/` | Chrome extension |

## Installation

### Homebrew (macOS)

```bash
brew install your-username/tap/nvim-web
```

### From Source

```bash
git clone https://github.com/your-username/nvim-web
cd nvim-web

# Build host
cd host && cargo build --release

# Build UI
cd ../ui && wasm-pack build --target web
```

### Chrome Extension

1. Open `chrome://extensions`
2. Enable Developer mode
3. Load unpacked: select `extension/` directory

### Neovim Plugin

```lua
-- lazy.nvim
{ "your-username/nvim-web.nvim" }

-- Or manually
vim.opt.runtimepath:append("/path/to/nvim-web/plugin")
require("nvim-web").setup()
```

## Usage

### Commands

| Command | Description |
|---------|-------------|
| `:NvimWebExplorer` | File explorer |
| `:NvimWebSessions` | Session manager |
| `:NvimWebSSH user@host` | Mount SSH |
| `:NvimWebConnections` | SSH manager |
| `:NvimWebConnect url` | Remote connect |

### URL Parameters

| Parameter | Description |
|-----------|-------------|
| `?session=new` | Force new session |
| `?session=<id>` | Join existing session |

## Configuration

### Host Config

`~/.config/nvim-web/config.toml`:

```toml
[server]
ws_port = 9001
bind = "127.0.0.1"

[session]
timeout = 300
```

### Plugin Config

```lua
require("nvim-web").setup({
  explorer_width = 30,
  explorer_position = "left",
  default_backend = "local",
})
```

## License

MIT
