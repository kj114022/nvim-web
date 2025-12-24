#!/bin/bash
# Run nvim-web host (builds if needed)
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Check if binary exists and is newer than source
BINARY="$SCRIPT_DIR/host/target/release/nvim-web-host"
MAIN_RS="$SCRIPT_DIR/host/src/main.rs"

if [ ! -f "$BINARY" ] || [ "$MAIN_RS" -nt "$BINARY" ]; then
    echo "Building nvim-web..."
    cd "$SCRIPT_DIR/host"
    cargo build --release --quiet
    echo ""
fi

exec "$BINARY" "$@"
