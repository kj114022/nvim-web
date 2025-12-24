#!/bin/bash
# Install nvim-web to /usr/local/bin for system-wide access
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
INSTALL_DIR="${1:-/usr/local/bin}"

echo "Building nvim-web..."
cd "$SCRIPT_DIR/host"
cargo build --release

echo ""
echo "Installing to $INSTALL_DIR..."

# Check if we need sudo
if [ -w "$INSTALL_DIR" ]; then
    cp "$SCRIPT_DIR/host/target/release/nvim-web-host" "$INSTALL_DIR/nvim-web"
    chmod +x "$INSTALL_DIR/nvim-web"
else
    echo "Need sudo access to install to $INSTALL_DIR"
    sudo cp "$SCRIPT_DIR/host/target/release/nvim-web-host" "$INSTALL_DIR/nvim-web"
    sudo chmod +x "$INSTALL_DIR/nvim-web"
fi

echo ""
echo "Installed successfully!"
echo ""
echo "Run 'nvim-web' from anywhere to start the server."
echo "Run 'nvim-web --help' for usage information."
