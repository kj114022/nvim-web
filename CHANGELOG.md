# Changelog

## [0.9.10] - 2026-01-14

### Infrastructure
- **De-Bazelification**: Completely removed Bazel build system in favor of standard Rust tooling.
- **Build System**: Introduced `Makefile` for unified host and WASM builds.
- **WASM**: Switched to `wasm-bindgen` CLI for more reliable artifact generation, bypassing `wasm-pack` metadata issues.
- **Dependencies**: Pinned `home`, `rmp-serde`, and `base64ct` to resolve `edition2024` compatibility issues.

### Fixes
- Fixed WASM compilation errors related to `home` crate platform mismatches.
- Removed legacy `WORKSPACE` and Bazel configuration files.
