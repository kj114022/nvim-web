#!/bin/bash
# Post-build script to patch WASM glue for Firefox compatibility
# The wasm-bindgen generated code uses ArrayBuffer.detached which requires Firefox 122+
# This script patches the check to use byteLength === 0 which works on all versions

set -e

JS_FILE="pkg/nvim_web_ui.js"

if [ ! -f "$JS_FILE" ]; then
    echo "Error: $JS_FILE not found"
    exit 1
fi

# Check if already patched
if grep -q "Firefox < 122" "$JS_FILE"; then
    echo "Already patched for Firefox compatibility"
    exit 0
fi

# Patch the detached property check to use byteLength instead
# Original: cachedDataViewMemory0.buffer.detached === true || (cachedDataViewMemory0.buffer.detached === undefined && ...)
# Fixed: cachedDataViewMemory0.byteLength === 0 || cachedDataViewMemory0.buffer !== wasm.memory.buffer

if grep -q "buffer\.detached === true" "$JS_FILE"; then
    # Use sed to replace the problematic line
    if [[ "$OSTYPE" == "darwin"* ]]; then
        # macOS sed requires empty string for -i
        sed -i '' 's/cachedDataViewMemory0\.buffer\.detached === true || (cachedDataViewMemory0\.buffer\.detached === undefined \&\& cachedDataViewMemory0\.buffer !== wasm\.memory\.buffer)/cachedDataViewMemory0.byteLength === 0 || cachedDataViewMemory0.buffer !== wasm.memory.buffer/g' "$JS_FILE"
        # Add comment before the function
        sed -i '' 's/function getDataViewMemory0() {/\/\/ Firefox < 122 compatibility: use byteLength instead of .buffer.detached\nfunction getDataViewMemory0() {/g' "$JS_FILE"
    else
        # Linux sed
        sed -i 's/cachedDataViewMemory0\.buffer\.detached === true || (cachedDataViewMemory0\.buffer\.detached === undefined \&\& cachedDataViewMemory0\.buffer !== wasm\.memory\.buffer)/cachedDataViewMemory0.byteLength === 0 || cachedDataViewMemory0.buffer !== wasm.memory.buffer/g' "$JS_FILE"
        sed -i 's/function getDataViewMemory0() {/\/\/ Firefox < 122 compatibility: use byteLength instead of .buffer.detached\nfunction getDataViewMemory0() {/g' "$JS_FILE"
    fi
    echo "Patched $JS_FILE for Firefox compatibility"
else
    echo "Warning: Expected pattern not found in $JS_FILE - may already be compatible or wasm-bindgen version changed"
fi
