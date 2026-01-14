.PHONY: all build test clean run build-host build-wasm

# Default target
all: build

# Build everything
build: build-host build-wasm

# Build the host binary (server)
build-host:
	cargo build --package nvim-web-host

# Build the WASM client (using wasm-bindgen directly due to wasm-pack issues)
build-wasm:
	cargo build --package nvim-web-ui --target wasm32-unknown-unknown --release
	wasm-bindgen target/wasm32-unknown-unknown/release/nvim_web_ui.wasm --out-dir pkg --target web --no-typescript

# Run tests
test:
	cargo test --workspace

# Clean artifacts
clean:
	cargo clean
	rm -rf pkg
