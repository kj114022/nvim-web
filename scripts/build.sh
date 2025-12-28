#!/usr/bin/env bash
#
# Build nvim-web workspace
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

echo "Building nvim-web workspace..."

# Build host (release)
echo "Building host..."
cargo build --release -p nvim-web-host

# Build UI (WASM)
echo "Building UI (WASM)..."
cd "${PROJECT_ROOT}/crates/ui"
wasm-pack build --target web --release

echo ""
echo "Build complete!"
echo "  Host binary: target/release/nvim-web-host"
echo "  WASM pkg:    crates/ui/pkg/"
