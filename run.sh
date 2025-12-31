#!/bin/bash
# Run nvim-web host (builds if needed)
set -e

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"

# Check if binary exists and is newer than source
BINARY="$SCRIPT_DIR/target/release/nvim-web-host"
CARGO_TOML="$SCRIPT_DIR/Cargo.toml"

if [ ! -f "$BINARY" ] || [ "$CARGO_TOML" -nt "$BINARY" ]; then
    echo "Building nvim-web..."
    cd "$SCRIPT_DIR"
    cargo build --release -p nvim-web-host --quiet
    echo ""
fi

exec "$BINARY" "$@"
