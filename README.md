# nvim-web

Neovim in the Browser - Full Neovim experience with WebSocket bridge.

## Quick Start

```bash
# Start the server (single binary - all assets embedded)
cd host && cargo build --release
./target/release/nvim-web-host

# Open browser
open http://localhost:8080
```

## Magic Link - Open Projects from Terminal

```bash
# Open any project directly in the browser
nvim-web open /path/to/project

# Open current directory
nvim-web open .
```

## Features

- Full Neovim rendering in canvas
- **Single binary** - UI assets embedded, no python server needed
- **Magic link** - Open projects from terminal with `nvim-web open`
- Complete keyboard and mouse support
- Session persistence across refreshes
- VFS with local filesystem access
- Real-time CWD and git branch sync
- Settings persistence (localStorage)
- PWA installable

## CLI Usage

```bash
nvim-web                  # Start server
nvim-web open [PATH]      # Open project in browser
nvim-web --help           # Show help
nvim-web --version        # Show version
```

## Keyboard Shortcuts

### Normal Mode
| Key | Action |
|-----|--------|
| `i` | Enter insert mode |
| `v` | Visual mode |
| `V` | Visual line mode |
| `:` | Command mode |
| `/` | Search forward |
| `?` | Search backward |
| `n` / `N` | Next/previous search result |
| `dd` | Delete line |
| `yy` | Yank (copy) line |
| `p` | Paste |
| `u` | Undo |
| `Ctrl+r` | Redo |
| `gg` | Go to top |
| `G` | Go to bottom |
| `:w` | Save file |
| `:q` | Quit |
| `:wq` | Save and quit |

### Insert Mode
| Key | Action |
|-----|--------|
| `Escape` | Return to normal mode |
| `Ctrl+c` | Return to normal mode |

### File Operations
| Command | Action |
|---------|--------|
| `:e <path>` | Open file |
| `:E @local/<path>` | Open from server filesystem |
| `:E @browser/<path>` | Open from browser OPFS |
| `:E @ssh/user@host/<path>` | Open from SSH remote |
| `:w` | Save current file |
| `:VfsStatus` | Show current VFS backend |
| `:!git status` | Run git status |
| `:!git diff` | Show git diff |

### File Browsing
| Command | Action |
|---------|--------|
| `:Ex` | Open netrw file explorer |
| `:Ex ~/path` | Browse specific directory |
| `:Vex` | Vertical split explorer |
| `h` / `l` | Navigate in explorer |
| `Enter` | Open file/directory |

## Project Configuration

Projects can have an optional `.nvim-web/config.toml`:

```toml
[project]
name = "My Project"

[editor]
cwd = "src"           # Working directory
init_file = "main.rs" # File to open on start
```

## Installation

### From Source

```bash
git clone https://github.com/your-username/nvim-web
cd nvim-web

# Build host (includes embedded UI)
cd host && cargo build --release

# Or build UI separately (requires wasm-pack)
cd ui && ./build.sh
```

### Requirements
- Rust 1.70+
- Neovim 0.9+
- wasm-pack (for UI build only)

## Configuration

### Browser Settings

Access via browser console:
```javascript
// Change font size
window.settings.set("fontSize", 16)

// Toggle theme
window.settings.set("theme", "dark")

// Reset to defaults
window.settings.reset()
```

### URL Parameters
| Parameter | Description |
|-----------|-------------|
| `?session=new` | Force new session |
| `?session=<id>` | Reconnect to session |
| `?open=<token>` | Open project via magic link |

## API Endpoints

| Endpoint | Method | Description |
|----------|--------|-------------|
| `/api/health` | GET | Health check |
| `/api/sessions` | GET | List sessions |
| `/api/open` | POST | Generate magic link token |
| `/api/claim/{token}` | GET | Claim magic link token |

## Troubleshooting

### UI Not Rendering

1. Check host is running: `curl http://localhost:8080`
2. Check browser console for errors (F12)
3. Clear service worker cache

### Keyboard Input Not Working

1. Click on the editor canvas to focus
2. Check if any browser extension is intercepting keys
3. Try incognito/private mode

### Session Not Reconnecting

1. Verify host is still running
2. Session may have timed out

## Architecture

```
Browser                     Host (Single Binary)
+-------------+            +-------------------+
| WASM UI     |  WebSocket | nvim-web-host     |
| (Canvas)    | <--------> | HTTP + WS Server  |
+-------------+            | Embedded UI       |
                           +--------+----------+
                                    |
                                    v
                           +-------------------+
                           | Neovim (--embed)  |
                           +-------------------+
```

## Components

| Directory | Description |
|-----------|-------------|
| `host/` | Rust host with embedded UI |
| `ui/` | WASM browser UI source |
| `plugin/` | Minimal Neovim VFS plugin |

## License

MIT
