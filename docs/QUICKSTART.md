# nvim-web Quick Start Guide

## Installation

### Pre-built Binary (Recommended)

```bash
# macOS (Apple Silicon)
curl -LO https://github.com/youruser/nvim-web/releases/latest/download/nvim-web-darwin-arm64
chmod +x nvim-web-darwin-arm64
sudo mv nvim-web-darwin-arm64 /usr/local/bin/nvim-web

# macOS (Intel)
curl -LO https://github.com/youruser/nvim-web/releases/latest/download/nvim-web-darwin-x64
chmod +x nvim-web-darwin-x64
sudo mv nvim-web-darwin-x64 /usr/local/bin/nvim-web

# Linux (x86_64)
curl -LO https://github.com/youruser/nvim-web/releases/latest/download/nvim-web-linux-x64
chmod +x nvim-web-linux-x64
sudo mv nvim-web-linux-x64 /usr/local/bin/nvim-web
```

### From Source

```bash
# Clone and build
git clone https://github.com/youruser/nvim-web.git
cd nvim-web
cargo build --release -p nvim-web-host

# Install
sudo cp target/release/nvim-web-host /usr/local/bin/nvim-web
```

## Running

### Basic Usage

```bash
# Start with default config
nvim-web

# With custom port
nvim-web --port 9000

# With TLS (wss://)
nvim-web --ssl-cert /path/to/cert.pem --ssl-key /path/to/key.pem
```

### Environment Variables

| Variable | Description |
|----------|-------------|
| `NVIM_WEB_TOKEN` | Authentication token |
| `GITHUB_TOKEN` | GitHub API token for `vfs://github` |
| `NVIM_WEB_TELEMETRY` | Enable crash reporting (`1` to enable) |

## Browser Access

Open your browser to:
- Local: `http://localhost:8080`
- With token: `http://localhost:8080?token=YOUR_TOKEN`
- View-only: `http://localhost:8080?viewer=1&session=SESSION_ID`

## Commands

### File Operations

| Command | Description |
|---------|-------------|
| `:E` | Open browser file picker |
| `:E path` | Open file directly |
| `:E @github/owner/repo/file.rs` | Open from GitHub |
| `:Edit` | Open browser file picker (alias) |

### Browser Integration

| Command | Description |
|---------|-------------|
| `:WebShare` | Copy session URL to clipboard |
| `:WebShare!` | Copy read-only viewer link |
| `:WebNotify msg` | Show browser notification |
| `:WebPrint` | Open browser print dialog |
| `:WebFullscreen` | Toggle fullscreen |
| `:WebViewers` | List connected viewers |

### Git Commands

| Command | Description |
|---------|-------------|
| `:Git <args>` | Run any git command |
| `:G <args>` | Alias for `:Git` |
| `:Gstatus` | Git status |
| `:Gdiff` | Git diff |
| `:Glog` | Git log (last 20) |
| `:Gblame` | Blame current file |
| `:Gadd` | Stage current file |
| `:Gcommit [msg]` | Commit changes |
| `:Gpush` | Push to remote |
| `:Gpull` | Pull from remote |

## Deployment

### Docker

```bash
docker run -p 8080:8080 nvim-web:latest
```

### Systemd

```ini
[Unit]
Description=nvim-web Neovim Browser Interface
After=network.target

[Service]
Type=simple
ExecStart=/usr/local/bin/nvim-web --port 8080
Restart=always
User=www-data

[Install]
WantedBy=multi-user.target
```

### Nginx Reverse Proxy

```nginx
server {
    listen 443 ssl;
    server_name nvim.example.com;
    
    location / {
        proxy_pass http://localhost:8080;
        proxy_http_version 1.1;
        proxy_set_header Upgrade $http_upgrade;
        proxy_set_header Connection "upgrade";
    }
}
```

## Keyboard Shortcuts

All standard Neovim keybindings work. Additional browser-specific:

| Key | Action |
|-----|--------|
| `Cmd+S` / `Ctrl+S` | Save file |
| Touch long-press | Right-click menu |
| Two-finger scroll | Scroll buffer |

## Troubleshooting

### Connection Issues
- Ensure port 8080 (or custom) is not in use
- Check firewall settings
- Try `wss://` if behind HTTPS proxy

### Rendering Issues
- Try Canvas2D fallback: add `?canvas=1` to URL
- Update browser to latest version

### Performance
- Large files (>1MB) are automatically truncated
- Use `:e path` instead of drag-drop for very large files
