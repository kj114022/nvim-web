# nvim-web justfile
# Run with: just <recipe>

# Default recipe - show available commands
default:
    @just --list

# Build all crates in release mode
build:
    cargo build --workspace --release

# Build in debug mode (faster compilation)
build-dev:
    cargo build --workspace

# Run the host server
run:
    cargo run -p nvim-web-host --release

# Run in development mode (debug build)
dev:
    cargo run -p nvim-web-host

# Run all tests
test:
    cargo test --workspace

# Run tests with output
test-verbose:
    cargo test --workspace -- --nocapture

# Run clippy with strict warnings
lint:
    cargo clippy --workspace -- -D warnings

# Format all code
fmt:
    cargo fmt --all

# Check formatting without modifying
fmt-check:
    cargo fmt --all -- --check

# Full CI check (format + lint + test)
ci: fmt-check lint test

# Build WASM UI (requires wasm-pack)
build-ui:
    cd crates/ui && wasm-pack build --target web --release

# Build UI in dev mode
build-ui-dev:
    cd crates/ui && wasm-pack build --target web --dev

# Clean all build artifacts
clean:
    cargo clean

# Install native messaging manifest for Chrome extension
install-native-manifest:
    ./scripts/install-native-manifest.sh

# Open project in browser via CLI
open path=".":
    cargo run -p nvim-web --release -- open {{path}}

# Watch and rebuild on changes (requires cargo-watch)
watch:
    cargo watch -x "build --workspace"

# Generate documentation
doc:
    cargo doc --workspace --no-deps --open

# Check compilation without building
check:
    cargo check --workspace

# Run benchmarks (if any)
bench:
    cargo bench --workspace

# Update dependencies
update:
    cargo update

# Show dependency tree
deps:
    cargo tree

# Security audit (requires cargo-audit)
audit:
    cargo audit
