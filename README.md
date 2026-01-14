# nvim-web

**Neovim in the browser.** No emulation, no compromises.
Runs your actual Neovim binary on a host machine and renders pixel-perfectly via WebAssembly.

---

## 🚀 Why nvim-web?

- **Zero Latency**: Uses WebTransport/QUIC for sub-50ms keystrokes.
- **Real Neovim**: Uses your existing config (`init.lua`), plugins, LSP, and Treesitter.
- **Collaborative**: Google Docs-style real-time editing with other users.
- **Secure**: OIDC authentication (Google, GitHub, Okta) and isolated sandboxing.
- **Universal**: Access your dev environment from any device (iPad, Chromebook, Laptop).

## ⚡ Quick Start

### 1. Install the Server
```bash
cargo install --git https://github.com/kj114022/nvim-web nvim-web-host
```

### 2. Run it
```bash
nvim-web
```

### 3. Connect
Open `http://localhost:8080` in your browser.

---

## 📦 Installation

### macOS (Homebrew)
```bash
brew install --build-from-source packaging/homebrew/homebrew.rb
```

### Ubuntu / Debian
```bash
wget https://github.com/kj114022/nvim-web/releases/download/v0.9.9/nvim-web_0.9.9_amd64.deb
sudo dpkg -i nvim-web_0.9.9_amd64.deb
```

### Fedora / RHEL
```bash
sudo dnf install nvim-web-0.9.9-1.x86_64.rpm
```

### Docker
```bash
docker run -p 8080:8080 -v $HOME/.config/nvim:/root/.config/nvim ghcr.io/kj114022/nvim-web
```

---

## ⚙️ Configuration

The server looks for a config file in:
- Linux: `~/.config/nvim-web/config.toml`
- macOS: `~/Library/Application Support/nvim-web/config.toml`

**Example `config.toml`:**
```toml
[server]
bind = "0.0.0.0"
port = 8080

[auth]
# Enable Google Login
provider = "google"
client_id = "..."
client_secret = "..."

[session]
# Persist sessions for 24 hours
timeout = 86400
```

---

## 🎮 Usage Guide

### Opening Projects
Run `nvim-web open` to start a session in a specific directory:
```bash
nvim-web open ~/code/my-project
```

### Remote Git Repositories
You can open a GitHub repo directly without cloning it locally first:
```bash
nvim-web open github.com/rust-lang/rust
```

### The "Universal Pipe"
One of nvim-web's most powerful features is the ability to pipe local browser tools into the remote Neovim instance.

**Example: Use a local LLM to fix code**
```lua
-- In Neovim
:ToolExec local-llm --prompt "Fix this bug" %
```

---

## ⌨️ Keybindings

- **`Cmd/Ctrl + P`**: Open file finder (native).
- **`Cmd/Ctrl + Shift + F`**: Global search (ripgrep).
- **`Alt + Click`**: Multi-cursor support.

---

## 🤝 Contributing

We love contributors! Please read [CONTRIBUTING.md](CONTRIBUTING.md) to get started.

## 📄 License

MIT