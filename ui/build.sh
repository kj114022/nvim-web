#!/bin/bash
# Build WASM UI with Firefox compatibility patch
set -e

cd "$(dirname "$0")"

echo "Building WASM..."
wasm-pack build --target web --out-dir pkg



echo "Build complete!"
