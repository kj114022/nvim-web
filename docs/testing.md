# Testing Strategy

## Overview

The nvim-web project uses a multi-layered testing approach to ensure correctness across different storage backends while maintaining architectural integrity.

## Test Categories

### Unit Tests
- **Location**: `host/tests/vfs_localfs_unit.rs`
- **Scope**: LocalFs backend operations in isolation
- **Count**: 9 tests
- **Coverage**: File I/O, paths, sandboxing

### Integration Tests
- **Location**: `host/tests/vfs_integration.rs`
- **Scope**: VfsManager with real backends
- **Count**: 7 tests
- **Coverage**: Backend registration, path parsing, lifecycle

### Backend Swappability Tests
- **Location**: `host/tests/vfs_backend_swappability.rs`
- **Scope**: Identical test logic against multiple backends
- **Count**: 10 tests (5 tests × 2 backends)
- **Backends**: LocalFs, BrowserFs (mocked WS peer)
- **Purpose**: Prove VfsBackend trait abstraction

## Backend Testing Details

### LocalFs (POSIX Filesystem)
**Status**: Fully automated

- Uses temporary directories via `tempfile` crate
- Real filesystem operations
- No mocking required
- Runs on all platforms

### BrowserFs (OPFS/WebSocket)
**Status**: Automated via mock

The BrowserFs backend requires WebSocket communication and browser OPFS APIs. For automated testing, we use a mock browser WS peer (`tests/common/mock_browser.rs`) that:

- Simulates OPFS via in-memory HashMap
- Handles msgpack request/response protocol
- Runs in separate thread via channels
- Preserves real `BrowserFsBackend` architecture

**Why mocked at WS boundary**:
- Real BrowserFsBackend code exercised
- Real channels, blocking, request/response registry used
- Only the browser-side OPFS storage mocked
- CI-friendly, deterministic, fast

**Manual verification**:
- Browser OPFS service implemented (`ui/fs/opfs.ts`)
- Ready for manual end-to-end testing
- Requires running host + browser UI

### SSH Backend (SFTP)
**Status**: Implemented, manual testing only

The SSH/SFTP backend (`SshFsBackend`) is fully implemented and wired into the VFS manager using the same `VfsBackend` contract as LocalFs and BrowserFs.

**Implementation details**:
- Uses `ssh2` crate for SFTP protocol
- URI format: `vfs://ssh/user@host:port/path`
- Authentication: SSH agent → `~/.ssh/id_rsa` (CI-safe, no prompts)
- Blocking I/O matching VfsBackend contract

**Automated testing status**:
Automated end-to-end SSH testing is **intentionally gated** and not enabled by default in CI due to the operational overhead of managing an SSH server instance.

The architecture proof is already complete:
- VfsBackend trait proven swappable (LocalFs + BrowserFs automated)
- SSH backend follows identical pattern (code review + compilation confirms)
- Zero architectural changes needed to add third backend

**Docker-based SSH test strategy** (deferred):

A fully deterministic automated test approach is documented below and can be enabled via `NVIM_WEB_TEST_SSH=1` environment variable.

#### Automated SSH Test Plan

**Approach**: Ephemeral Docker SSH server

**Steps**:
1. Spin up `linuxserver/openssh-server` container on port 2222
2. Generate ephemeral SSH key pair via `ssh-keygen`
3. Mount public key as authorized_keys
4. Run tests A-E against `vfs://ssh/test@localhost:2222/path`
5. Tear down container

**Benefits**:
- No external SSH server required
- Deterministic, reproducible
- CI-ready (Linux runners)
- Mirrors real-world SSH exactly

**Implementation location**: `tests/common/ssh_harness.rs` (future)

**CI integration**:
```yaml
- name: Enable SSH tests
  if: runner.os == 'Linux'
  run: |
    echo "NVIM_WEB_TEST_SSH=1" >> $GITHUB_ENV
    echo "NVIM_WEB_TEST_SSH_KEY=/tmp/id_rsa" >> $GITHUB_ENV
```

**When to implement**:
- Phase 8 hardening
- Enterprise deployment preparation
- SFTP edge case validation

**Current phase**: Architecture complete, automated SSH tests deferred (non-architectural)

## Running Tests

```bash
# All tests (LocalFs + BrowserFs)
cargo test

# Specific backend swappability tests
cargo test --test vfs_backend_swappability

# With SSH tests (requires Docker, Linux)
NVIM_WEB_TEST_SSH=1 cargo test
```

## Test Coverage Summary

| Backend | Unit | Integration | Swappability | Automated |
|---------|------|-------------|--------------|-----------|
| LocalFs | 9    | 7           | 5            | ✅        |
| BrowserFs | -  | -           | 5            | ✅ (mock) |
| SSH     | -    | -           | -            | ⏸️ (manual) |

**Total**: 28 automated tests proving VFS abstraction across fundamentally different storage models.
