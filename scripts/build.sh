#!/usr/bin/env bash
#
# Build nvim-web workspace
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

echo "Building nvim-web workspace..."

# Build UI (WASM)
echo "Building UI (WASM)..."
cd "${PROJECT_ROOT}/crates/ui"

# Compile TypeScript to JavaScript (if tsc available)
if command -v npx &> /dev/null && [ -f "tsconfig.json" ]; then
  echo "Compiling TypeScript..."
  npx tsc  # Generates JS in dist/
  
  # Copy compiled artifacts to root (replacing old files)
  cp dist/sw.js .
  cp dist/worker.js .
  cp manifest.json dist/
  cp -r public/icons dist/
  cp "${PROJECT_ROOT}/config.js" dist/
  
  # Recreate fs/ structure for WASM/SW compatibility
  mkdir -p fs
  cp dist/fs_driver.js fs/
  cp dist/opfs.js fs/
  cp dist/session_storage.js fs/
fi

wasm-pack build --target web --release

# Strip WASM binary (reduce size)
if command -v wasm-strip &> /dev/null; then
  echo "Stripping WASM binary..."
  wasm-strip pkg/nvim_web_ui_bg.wasm
fi

# Build host (release)
echo "Building host..."
cd "${PROJECT_ROOT}"
cargo build --release -p nvim-web-host

echo ""
echo "Build complete!"
echo "  Host binary: target/release/nvim-web-host"
echo "  WASM pkg:    crates/ui/pkg/"

