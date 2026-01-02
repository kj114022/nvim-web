# Contributing to nvim-web

First off, thank you for considering contributing to nvim-web. This project is a rigorous implementation of Neovim in the browser, adhering to strict engineering principles.

## 1. Engineering Philosophy

We follow a production-grade engineering ruleset. Please review the following core principles before writing code:

- **Correctness First**: Speed and features are secondary to correctness.
- **Rust-Centric**: We allow the compiler to guide design. `clippy` is law.
- **No Compromises**: We do not emulate functionality if we can wrap the real thing.
- **Minimal Dependencies**: Audit every crate. We prefer depth over breadth.

## 2. Architecture Overview

`nvim-web` is a monorepo organized into workspaces:

| Crate | Path | Description |
|-------|------|-------------|
| **nvim-web-host** | `crates/host` | Rust backend (Axum/Tokio). Handles WebSocket, Neovim process management, and VFS ops. |
| **nvim-web-ui** | `crates/ui` | Frontend (Rust/WASM). Renders Neovim grid to HTML5 Canvas. |
| **nvim-web-vfs** | `crates/vfs` | Virtual Filesystem abstraction (Local, Browser/OPFS, SSH, Git). |
| **nvim-web-protocol** | `crates/protocol` | Shared MessagePack types and constants. |

## 3. Development Setup

### Prerequisites
- **Rust**: Latest stable (v1.75+).
- **Neovim**: v0.9.0 or higher (must be in `$PATH`).
- **wasm-pack**: For building the UI (`cargo install wasm-pack`).
- **Docker**: Optional, for running SSH integration tests.

### Build Instructions

1. **Clone the repository**:
   ```bash
   git clone https://github.com/kj114022/nvim-web.git
   cd nvim-web
   ```

2. **Build the Wasm UI**:
   The UI must be compiled to WASM before the host can embed it.
   ```bash
   cd crates/ui
   wasm-pack build --target web --release
   cd ../..
   ```

3. **Build the Host**:
   ```bash
   cargo build -p nvim-web-host
   ```

4. **Run Locally**:
   ```bash
   # Runs the host, which serves the compiled WASM UI
   cargo run -p nvim-web-host
   ```
   Open `http://127.0.0.1:8080`.

## 4. Testing

We expect all tests to pass before merging.

### Unit Tests
```bash
cargo test
```

### Integration Tests (SSH)
Requires Docker to spin up a test SSH server.
```bash
docker-compose up -d ssh-test
cargo test --test ssh_integration
```

## 5. Coding Standards

### Rust
- **Formatting**: We use `rustfmt` with a custom config. Run `cargo fmt` before committing.
- **Linting**: We use strict `clippy` settings. Run `cargo clippy --all-targets` and ensure it is clean.
- **Async**: Use `tokio`. Avoid blocking threads in async contexts.
- **Error Handling**: Use `anyhow` for applications, `thiserror` for libraries. Propagate errors; do not `unwrap()` in production code.

### Git & Commits
- Use [Conventional Commits](https://www.conventionalcommits.org/).
  - `feat: add overlay filesystem`
  - `fix: resolve websocket reconnection issue`
  - `docs: update protocol specification`
- Keep commits atomic. One logical change per commit.
- Do not include `target/` or `pkg/` artifacts in commits.

## 6. Submission Guidelines

1. **Fork** the repo on GitHub.
2. **Clone** your fork locally.
3. **Create a branch** for your feature (`git checkout -b feat/my-feature`).
4. **Implement**, verifying with tests.
5. **Format & Lint**: `cargo fmt && cargo clippy`.
6. **Commit** using conventional messages.
7. **Push** and create a **Pull Request**.

## 7. Protocol Documentation

If you modify the communication protocol (RPC messages, notification types), you **MUST** update `docs/protocol.md` in the same PR. The documentation is the source of truth for the wire format.
