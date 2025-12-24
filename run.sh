#!/bin/bash
# Build and run nvim-web host
set -e

cd "$(dirname "$0")"

echo "Building nvim-web host..."
cd host
cargo build --release

echo ""
echo "Starting nvim-web host..."
exec ./target/release/nvim-web-host "$@"
