# nvim-web

**Neovim in the browser.** No emulation, no compromises.
Run your native Neovim binary on a secure host and render pixel-perfectly via WebAssembly.

## The Paradigm Shift

nvim-web is not another terminal emulator. It is a **distributed rendering protocol** that decouples the compute (host) from the display (client).

- **Zero Latency**: Powered by **WebTransport/QUIC**, delivering sub-50ms keystroke latency globally.
- **Native Integrity**: Preserves 100% of your existing Neovim configuration (`init.lua`), plugins, LSP, and Treesitter.
- **Real-Time Synchronization**: Multi-user collaboration with eventual consistency guaranteed by **CRDTs (y-crdt)**.
- **Enterprise Security**: OIDC-compliant authentication (Google, Okta, Azure AD) with per-session container isolation.
- **Universal Access**: A full-fidelity development environment on any browser-capable device.

---

## Quick Start
### 1. Install Host
```bash
cargo install --git https://github.com/kj114022/nvim-web nvim-web-host
```

### 2. Launch Server
```bash
nvim-web
```

### 3. Connect
Navigate to `http://localhost:8080`.

---

## Installation Methods

### macOS (Homebrew)
```bash
brew install --build-from-source packaging/homebrew/homebrew.rb
```

### Linux (Debian/Ubuntu)
```bash
wget https://github.com/kj114022/nvim-web/releases/download/v0.9.9/nvim-web_0.9.9_amd64.deb
sudo dpkg -i nvim-web_0.9.9_amd64.deb
```

### Linux (Fedora/RHEL)
```bash
sudo dnf install nvim-web-0.9.9-1.x86_64.rpm
```

### Docker
```bash
docker run -p 8080:8080 -v $HOME/.config/nvim:/root/.config/nvim ghcr.io/kj114022/nvim-web
```

### Hermetic Build (Bazel)
For enterprise environments requiring reproducible builds:
```bash
bazel build //crates/host:nvim-web-host --compilation_mode=opt
```

---

## CLI Reference

The `nvim-web` binary orchestrates the host environment.

### Server Routing
```bash
# Start on default port (127.0.0.1:8080)
nvim-web

# Bind to 0.0.0.0 for external access
nvim-web --bind 0.0.0.0 --port 3000

# Enforce secure token authentication
NVIM_WEB_TOKEN="secure-token-123" nvim-web
```

### Session Orchestration
Launch ephemeral or persistent sessions:

```bash
# Open a local project directory
nvim-web open ~/code/my-project

# Open a specific file and jump to line
nvim-web open ~/src/main.rs:105
```

### Cloud Repositories
Instantly clone and mount remote Git repositories in an ephemeral sandbox:

```bash
# Public Repository
nvim-web open github.com/rust-lang/rust

# Private Repository (via SSH agent forwarding)
nvim-web open git@github.com:corp/proprietary-core.git
```

---

## Enhanced Feature Ecosystem

### 1. Backend Hot-Swap
Seamlessly migrate your live session between different compute nodes without losing cursor position or unsaved changes.

- **Local**: Your machine (`local`)
- **Docker**: A containerized environment (`docker:android-build-env`)
- **SSH**: A remote bare-metal server (`ssh://user@dev-box`)
- **TCP**: A raw TCP socket to another nvim-web instance (`tcp://10.0.0.5:6666`)

**Usage**:
Use the command palette (`Ctrl+Shift+P`) -> "Swap Backend" or in Lua:
```lua
-- Move entire session to a Docker container
require("nvim-web.backend").swap("docker:rust-env")
```

### 2. P2P Encrypted Chat
Built-in WebRTC mesh network for developer communication.
- **End-to-End Encrypted (DTLS)**: The server only relays signals; it cannot read messages.
- **No Database**: Messages are ephemeral and live only in browser memory.
- **Usage**: Click the "Chat" icon in the sidebar to invite peers.

### 3. Universal Tool Pipe
Bridge the gap between local client tools and the remote host environment. Pipe buffer content to local LLMs, formatters, or linters securely.

**Vim Command**:
```vim
" Pipe current buffer to local 'claude' CLI
:ToolExec claude --prompt "Optimize this generic" %
```

**Lua API**:
```lua
local pipe = require("nvim-web.pipe")
-- invoke local 'prettier' and replace buffer content
pipe.exec("prettier", { "--stdin-filepath", vim.api.nvim_buf_get_name(0) }, buffer_content)
```

### 4. Virtual Filesystem (VFS) Layer
Mount remote storage backends transparently. The VFS layer abstracts the underlying storage protocol so Neovim sees a normal filesystem.

| VFS Scheme | Syntax | Example |
|------------|--------|---------|
| **Local** | `local://` | `local:///home/dev/src` |
| **SSH** | `ssh://` | `ssh://deploy@10.0.1.5:/var/www` |
| **Git** | `git://` | `git://github.com/org/repo.git` |
| **GitHub** | `github://` | `github://owner/repo` |
| **SFTP** | `sftp://` | `sftp://user@storage-box:/data` |

### 5. Host-Side Search (Ripgrep)
Offloads heavy search operations to the host machine for native performance, bypassing WASM memory limits.
- **Command**: `Cmd/Ctrl + Shift + F`
- **Engine**: `ripgrep` (binary must be in PATH)
- **Performance**: Searches 1GB+ logs in milliseconds.

### 6. Identity & Access (OIDC)
Secure your instance with industry-standard OAuth2 providers.

**Configuration (`config.toml`)**:
```toml
[auth]
provider = "google"
client_id = "YOUR_CLIENT_ID"
client_secret = "YOUR_CLIENT_SECRET"
redirect_url = "https://nvim.corp.com/auth/callback"
allowed_domains = ["corp.com"]
```

### 7. Chrome Extension
For specific workflows, install the browser extension located in `extension/`:
1. Open `chrome://extensions`
2. Enable "Developer Mode"
3. "Load Unpacked" -> select `extension/` folder.
4. **Feature**: One-click to open any GitHub URL in nvim-web.

---

## Enterprise Scenarios

### Secure Remote Development
**Challenge**: Source code cannot leave the VPC, but engineers need a high-fidelity local experience.
**Solution**: Deploy `nvim-web` inside the VPC. Code remains on the server; only encrypted WebSocket frames leave the network. Zero data exfiltration risk.

### Instant Onboarding
**Challenge**: New engineers spend days configuring local environments.
**Solution**: Provide a central `nvim-web` url. New hires authenticate via SSO and drop immediately into a fully configured, compliant development environment.

### Low-Latency Pair Programming
**Challenge**: Screen sharing introduces input lag and resolution artifacts.
**Solution**: `nvim-web` collaboration. Both engineers edit the same AST in real-time with local latency, enabling true "distributed pair programming".

---

## Native Bindings

- **`Cmd/Ctrl + P`**: Quick File Finder (Native UI).
- **`Cmd/Ctrl + Shift + F`**: Global Project Search (Host-side).
- **`Alt + Click`**: Spawn multiple cursors (Multicursor engine).
- **`Ctrl + ~`**: Toggle Integrated Terminal panel.

---

## License

MIT