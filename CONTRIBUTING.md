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
| **nvim-web-host** | `crates/host` | Rust backend (Axum/Tokio). WebSocket, WebTransport, Neovim process, VFS, auth, K8s. |
| **nvim-web-ui** | `crates/ui` | Frontend (Rust/WASM). Renders Neovim grid to HTML5 Canvas. |
| **nvim-web-ui-window** | `crates/ui-window` | Main thread WASM module. |
| **nvim-web-ui-worker** | `crates/ui-worker` | Web Worker WASM module. |
| **nvim-web-vfs** | `crates/vfs` | Virtual Filesystem abstraction (Local, Browser/OPFS, SSH, Git). |
| **nvim-web-protocol** | `crates/protocol` | Shared MessagePack types and constants. |

### Host Modules

| Module | Description |
|--------|-------------|
| `transport/` | WebSocket and WebTransport abstraction |
| `crdt/` | y-crdt based collaborative editing |
| `oidc/` | OpenID Connect authentication with BeyondCorp policies |
| `k8s/` | Kubernetes pod-per-session management |
| `collaboration/` | Multi-user session support with CRDTs |

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

2. **Build the WASM UI**:
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

### Browser Tests (Playwright)
```bash
npx playwright test
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

## 7. Documentation

When modifying features, update the corresponding documentation:

| Feature | Documentation |
|---------|---------------|
| Protocol | `docs/protocol.md` |
| WebTransport | `docs/webtransport.md` |
| Collaboration | `docs/collaboration.md` |
| Authentication | `docs/authentication.md` |
| Kubernetes | `docs/kubernetes.md` |

## 8. Code Review

All PRs require at least one approval. Reviewers check for:

- Correctness and test coverage
- Consistent coding style
- Documentation updates
- Performance implications
- Security considerations
