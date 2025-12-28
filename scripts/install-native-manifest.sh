#!/usr/bin/env bash
#
# install-native-manifest.sh
# Installs the Native Messaging Host manifest for Chrome/Firefox
#

set -e

PROJECT_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HOST_BINARY="$PROJECT_ROOT/target/release/nvim-web-host"
MANIFEST_NAME="com.kj114022.nvim_web"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
RED='\033[0;31m'
NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[SUCCESS]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

# 1. Ensure binary exists (warn if debug)
if [ ! -f "$HOST_BINARY" ]; then
    log_info "Release binary not found at $HOST_BINARY"
    log_info "Trying debug binary..."
    HOST_BINARY="$PROJECT_ROOT/target/debug/nvim-web-host"
    if [ ! -f "$HOST_BINARY" ]; then
        log_error "Host binary not found! Please run ./scripts/build.sh first."
        exit 1
    fi
fi

# 2. Determine Installation Path
OS="$(uname -s)"
if [ "$OS" = "Darwin" ]; then
    # macOS
    CHROME_DIR="$HOME/Library/Application Support/Google/Chrome/NativeMessagingHosts"
    FIREFOX_DIR="$HOME/Library/Application Support/Mozilla/NativeMessagingHosts"
elif [ "$OS" = "Linux" ]; then
    # Linux
    CHROME_DIR="$HOME/.config/google-chrome/NativeMessagingHosts"
    FIREFOX_DIR="$HOME/.mozilla/native-messaging-hosts"
else
    log_error "Unsupported OS: $OS"
    exit 1
fi

mkdir -p "$CHROME_DIR"
# mkdir -p "$FIREFOX_DIR" # Uncomment for FF support

# 3. Create Manifest JSON content
# Must match extension ID in Chrome
EXTENSION_ID="hpofmnjplmjnfhlnpepjnkhdfhfhepcg" # Generated from key in extension/
# If extending, replace with dynamic ID or allow-listing active development ID

if [ -f "$PROJECT_ROOT/extension/manifest.json" ]; then
    # Note: Extension ID is determined by the key in the manifest or assigned by Chrome Store.
    # For local dev, we often load unpacked and copy the ID.
    log_info "Using Extension ID: $EXTENSION_ID (Update this if it changes!)"
fi

cat > "$CHROME_DIR/$MANIFEST_NAME.json" <<EOF
{
  "name": "$MANIFEST_NAME",
  "description": "nvim-web Native Host",
  "path": "$HOST_BINARY",
  "type": "stdio",
  "allowed_origins": [
    "chrome-extension://$EXTENSION_ID/"
  ]
}
EOF

log_success "Installed Chrome manifest to $CHROME_DIR/$MANIFEST_NAME.json"
log_info "Pointing to binary: $HOST_BINARY"

# 4. Create wrapper script if needed (not needed for Rust binary usually, but good for env vars)
# For now, we point directly to binary. Chrome passes arguments? 
# Chrome runs: "/path/to/binary" chrome-extension://id/
# We need to detect if we are being run by Chrome.
# But our binary currently requires `--native`. 
# We can't easily pass args in the manifest "path" (it must be an absolute path).
# SOLUTION: Create a wrapper script that passes --native.

WRAPPER_SCRIPT="$PROJECT_ROOT/target/nvim-web-native-wrapper"
cat > "$WRAPPER_SCRIPT" <<EOF
#!/bin/bash
# Wrapper to launch nvim-web-host with --native flag
"$HOST_BINARY" --native "\$@"
EOF
chmod +x "$WRAPPER_SCRIPT"

# Update manifest to point to wrapper
# Using sed to replace path line
sed -i '' "s|\"path\": .*|\"path\": \"$WRAPPER_SCRIPT\",|" "$CHROME_DIR/$MANIFEST_NAME.json"

log_success "Created wrapper script at $WRAPPER_SCRIPT"
log_success "Updated manifest to use wrapper."
