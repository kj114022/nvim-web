#!/bin/bash
# Install nvim-web to /usr/local/bin for system-wide access
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
INSTALL_DIR="${1:-/usr/local/bin}"
CONFIG_DIR="${XDG_CONFIG_HOME:-$HOME/.config}/nvim-web"

# Check dependencies
if ! command -v cargo &> /dev/null; then
    echo "Error: 'cargo' is not installed. Please install Rust: https://rustup.rs/"
    exit 1
fi

if ! command -v nvim &> /dev/null; then
    echo "Warning: 'nvim' is not in PATH. nvim-web requires Neovim 0.9+."
fi

echo "Building nvim-web (release)..."
cd "$SCRIPT_DIR"
if ! cargo build --release -p nvim-web-host; then
    echo "Error: Build failed."
    exit 1
fi

echo ""
echo "Installing to $INSTALL_DIR..."

SOURCE_BIN="$SCRIPT_DIR/target/release/nvim-web-host"
TARGET_BIN="$INSTALL_DIR/nvim-web"

# Check permissions
if [ -w "$INSTALL_DIR" ]; then
    cp "$SOURCE_BIN" "$TARGET_BIN"
    chmod +x "$TARGET_BIN"
else
    echo "Requesting sudo access to install to $INSTALL_DIR..."
    sudo cp "$SOURCE_BIN" "$TARGET_BIN"
    sudo chmod +x "$TARGET_BIN"
fi

# Create config directory and example config
mkdir -p "$CONFIG_DIR"
if [ ! -f "$CONFIG_DIR/config.toml" ]; then
    if [ -f "$SCRIPT_DIR/config.example.toml" ]; then
        cp "$SCRIPT_DIR/config.example.toml" "$CONFIG_DIR/config.toml"
        echo "Created config at: $CONFIG_DIR/config.toml"
    fi
fi

echo ""
echo "Installation complete!"
echo ""
echo "Run 'nvim-web' to start the server."
echo "Run 'nvim-web --help' for options."
echo ""
echo "Configuration: $CONFIG_DIR/config.toml"
echo "Documentation: docs/*.md"
