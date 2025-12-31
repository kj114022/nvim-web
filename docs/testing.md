# Testing Philosophy

> "Random testing plays as important a role as formal verification in the future of software engineering."
> — Alperen Keles, "Test, don't (just) verify"

## Core Principle: Test + Verify

nvim-web embraces a **dual strategy**:

1. **Verification**: Type-safe contracts via Rust's type system and trait bounds
2. **Testing**: Random/property-based testing to catch what verification misses

Neither is sufficient alone. Proofs guarantee invariants; tests reveal real-world failures.

## Verification-Guided Development (VGD)

We apply VGD principles where practical:

| Layer | Verified Reference | Production Code | Differential Test |
|-------|-------------------|-----------------|-------------------|
| VFS   | `VfsBackend` trait | `LocalFs`, `BrowserFs`, `SshFs` | Backend swappability tests |
| Protocol | `protocol.md` spec | `ws.rs` message handling | Roundtrip encoding tests |
| Grid | Cell invariants | `GridManager` | Render consistency checks |

The verified reference (trait definition + spec) is simple and provably correct.
The production code is complex and fast.
Differential testing bridges them.

---

## Test Categories

### Unit Tests

| Location | Scope | Count |
|----------|-------|-------|
| `host/tests/vfs_localfs_unit.rs` | LocalFs operations | 9 |

Focus: File I/O, path handling, sandbox escape prevention.

### Integration Tests

| Location | Scope | Count |
|----------|-------|-------|
| `host/tests/vfs_integration.rs` | VfsManager lifecycle | 7 |

Focus: Backend registration, URI parsing, error propagation.

### Backend Swappability (Differential)

| Location | Scope | Count |
|----------|-------|-------|
| `host/tests/vfs_backend_swappability.rs` | Trait conformance | 10 |

**This is our VGD implementation.** The same test logic runs against multiple backends:
- `LocalFs` (real filesystem)
- `MemoryFs` (in-memory emulation)
- `BrowserFs` (mocked WebSocket peer)

If all pass with identical assertions, the trait abstraction is proven correct.

---

## Backend Testing Details

### LocalFs (Reference Implementation)

- Uses `tempfile` for ephemeral directories
- Real POSIX operations
- **Verified**: Trait contract satisfied by direct implementation

### BrowserFs (Production Complexity)

- Requires WebSocket + OPFS (browser APIs)
- Tested via mock peer (`tests/common/mock_browser.rs`)
- **Differential**: Must produce identical results to LocalFs for same inputs

### SSH Backend (Deferred)

- Implementation complete, integration tests active in CI
- Docker-based automated tests (`pytest` style logic in Rust)
- Trait conformance proven by code structure (same pattern as LocalFs/BrowserFs)

---

## Running Tests

```bash
# All tests
cargo test --workspace

# VFS differential tests only
cargo test --test vfs_backend_swappability

# Enable SSH tests (requires Docker, Linux)
NVIM_WEB_TEST_SSH=1 cargo test
```

---

## What Tests Catch That Verification Cannot

| Category | Example | Why Verification Fails |
|----------|---------|----------------------|
| Performance regressions | Slow reconnection | No performance model |
| Browser quirks | Firefox OPFS gaps | Environment-specific |
| Network failures | WebSocket timeouts | Non-deterministic I/O |
| User behavior | Rapid refresh cycles | Unpredictable input |

These require empirical observation, not proofs.

---

## Test Coverage Summary

| Backend | Unit | Integration | Differential | Status |
|---------|------|-------------|--------------|--------|
| LocalFs | 9 | 7 | 5 | Automated |
| MemoryFs | 4 | - | 5 | Automated |
| OverlayFs | - | 3 | - | Automated |
| BrowserFs | - | - | 5 | Automated (mock) |
| SSH | - | 1 | - | Automated (Docker) |

**Total**: 40+ automated tests proving VFS abstraction correctness.

## CI/CD Pipeline

The GitHub Actions workflow ensures rigorous verification:

1. **Test Matrix**: Stable vs Nightly Neovim
2. **SSH Integration**: Spins up `linuxserver/openssh-server` container for real end-to-end SFTP testing
3. **WASM Build**: Verifies `nvim-web-ui` compiles to valid WASM
4. **Release**: Builds and uploads static binaries for tagged releases
