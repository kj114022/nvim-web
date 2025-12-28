#!/usr/bin/env bash
#
# nvim-web: Migrate to Cargo Workspace
# Phase 1: Foundation Setup
#
# This script converts the project from separate Rust projects
# to a unified Cargo workspace structure.
#
# Safety features:
#   - Strict mode (fail on any error)
#   - Pre-flight checks before any changes
#   - Git history preserved via git mv
#   - Dry-run mode available
#   - Rollback instructions provided
#
# Usage:
#   ./scripts/migrate-to-workspace.sh          # Execute migration
#   ./scripts/migrate-to-workspace.sh --dry-run # Preview changes only
#   ./scripts/migrate-to-workspace.sh --help    # Show help
#

set -euo pipefail
IFS=$'\n\t'

# -----------------------------------------------------------------------------
# Configuration
# -----------------------------------------------------------------------------

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
readonly CRATES_DIR="${PROJECT_ROOT}/crates"
readonly BACKUP_BRANCH="backup/pre-restructure-2024-12-28"
readonly TAG_NAME="v0.1.0-pre-restructure"

# Colors for output
readonly RED='\033[0;31m'
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly BLUE='\033[0;34m'
readonly NC='\033[0m' # No Color

# Dry run mode
DRY_RUN=false

# -----------------------------------------------------------------------------
# Logging Functions
# -----------------------------------------------------------------------------

log_info() {
    echo -e "${BLUE}[INFO]${NC} $1"
}

log_success() {
    echo -e "${GREEN}[OK]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1" >&2
}

log_step() {
    echo -e "\n${GREEN}==>${NC} ${BLUE}$1${NC}"
}

# -----------------------------------------------------------------------------
# Helper Functions
# -----------------------------------------------------------------------------

run_cmd() {
    if [[ "${DRY_RUN}" == true ]]; then
        echo -e "${YELLOW}[DRY-RUN]${NC} $*"
    else
        "$@"
    fi
}

check_command() {
    if ! command -v "$1" &> /dev/null; then
        log_error "Required command '$1' not found"
        exit 1
    fi
}

# -----------------------------------------------------------------------------
# Pre-flight Checks
# -----------------------------------------------------------------------------

preflight_checks() {
    log_step "Running pre-flight checks"

    # Check required commands
    check_command git
    check_command cargo
    check_command wasm-pack
    log_success "Required commands available"

    # Check we're in the project root
    if [[ ! -f "${PROJECT_ROOT}/README.md" ]]; then
        log_error "Not in nvim-web project root"
        exit 1
    fi
    log_success "Project root confirmed"

    # Check we're on the restructure branch
    local current_branch
    current_branch=$(git -C "${PROJECT_ROOT}" branch --show-current)
    if [[ "${current_branch}" != "feature/restructure-architecture" ]]; then
        log_error "Not on feature/restructure-architecture branch (on: ${current_branch})"
        log_info "Run: git checkout feature/restructure-architecture"
        exit 1
    fi
    log_success "On correct branch: ${current_branch}"

    # Check working directory is clean
    if [[ -n "$(git -C "${PROJECT_ROOT}" status --porcelain)" ]]; then
        log_warn "Working directory has uncommitted changes"
        log_info "Changes will be included in migration commit"
    else
        log_success "Working directory clean"
    fi

    # Check backup exists
    if ! git -C "${PROJECT_ROOT}" rev-parse --verify "${BACKUP_BRANCH}" &> /dev/null; then
        log_error "Backup branch '${BACKUP_BRANCH}' not found"
        exit 1
    fi
    log_success "Backup branch exists: ${BACKUP_BRANCH}"

    # Check tag exists
    if ! git -C "${PROJECT_ROOT}" rev-parse --verify "${TAG_NAME}" &> /dev/null; then
        log_error "Backup tag '${TAG_NAME}' not found"
        exit 1
    fi
    log_success "Backup tag exists: ${TAG_NAME}"

    # Check current structure
    if [[ ! -d "${PROJECT_ROOT}/host" ]]; then
        log_error "host/ directory not found - already migrated?"
        exit 1
    fi
    if [[ ! -d "${PROJECT_ROOT}/ui" ]]; then
        log_error "ui/ directory not found - already migrated?"
        exit 1
    fi
    log_success "Current structure verified (host/, ui/ exist)"

    # Check crates/ doesn't exist yet
    if [[ -d "${CRATES_DIR}" ]]; then
        log_error "crates/ directory already exists - already migrated?"
        exit 1
    fi
    log_success "crates/ directory does not exist (expected)"

    echo ""
    log_success "All pre-flight checks passed"
}

# -----------------------------------------------------------------------------
# Migration Steps
# -----------------------------------------------------------------------------

step_1_create_crates_dir() {
    log_step "Step 1: Creating crates/ directory"
    run_cmd mkdir -p "${CRATES_DIR}"
    log_success "Created ${CRATES_DIR}"
}

step_2_move_host() {
    log_step "Step 2: Moving host/ to crates/host/"
    
    # Use git mv to preserve history
    run_cmd git -C "${PROJECT_ROOT}" mv host "${CRATES_DIR}/host"
    log_success "Moved host/ to crates/host/"
}

step_3_move_ui() {
    log_step "Step 3: Moving ui/ to crates/ui/"
    
    # Use git mv to preserve history
    run_cmd git -C "${PROJECT_ROOT}" mv ui "${CRATES_DIR}/ui"
    log_success "Moved ui/ to crates/ui/"
}

step_4_move_lint_configs() {
    log_step "Step 4: Moving lint configs to root"
    
    # Move clippy.toml and rustfmt.toml to root if they exist in host
    if [[ -f "${CRATES_DIR}/host/clippy.toml" ]]; then
        run_cmd git -C "${PROJECT_ROOT}" mv "${CRATES_DIR}/host/clippy.toml" "${PROJECT_ROOT}/clippy.toml"
        log_success "Moved clippy.toml to root"
    fi
    
    if [[ -f "${CRATES_DIR}/host/rustfmt.toml" ]]; then
        run_cmd git -C "${PROJECT_ROOT}" mv "${CRATES_DIR}/host/rustfmt.toml" "${PROJECT_ROOT}/rustfmt.toml"
        log_success "Moved rustfmt.toml to root"
    fi
}

step_5_create_root_cargo_toml() {
    log_step "Step 5: Creating root Cargo.toml (virtual manifest)"
    
    if [[ "${DRY_RUN}" == true ]]; then
        echo -e "${YELLOW}[DRY-RUN]${NC} Would create ${PROJECT_ROOT}/Cargo.toml"
        return
    fi
    
    cat > "${PROJECT_ROOT}/Cargo.toml" << 'CARGO_TOML'
# nvim-web: Neovim in the Browser
# Cargo Workspace Configuration

[workspace]
resolver = "2"
members = [
    "crates/host",
    "crates/ui",
]

[workspace.package]
version = "0.1.0"
edition = "2021"
license = "MIT"
repository = "https://github.com/kj114022/nvim-web"
homepage = "https://github.com/kj114022/nvim-web"
readme = "README.md"
keywords = ["neovim", "browser", "websocket", "wasm", "editor"]
categories = ["development-tools", "web-programming"]

# Shared dependencies - use `package = { workspace = true }` in member crates
[workspace.dependencies]
# Async runtime
tokio = { version = "1", features = ["full"] }
futures = "0.3"
async-trait = "0.1"

# Serialization
serde = { version = "1", features = ["derive"] }
rmpv = "1"
rmp-serde = "1"
serde_json = "1"

# WebSocket
tokio-tungstenite = "0.24"
tungstenite = "0.24"

# HTTP server
axum = "0.7"
tower-http = { version = "0.5", features = ["cors"] }
http = "1.4.0"

# WASM
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
js-sys = "0.3"
web-sys = "0.3"

# Error handling
anyhow = "1"

# Utilities
lazy_static = "1.4"
dirs = "5"
rand = "0.8"
mime_guess = "2"

# Workspace-level lints
[workspace.lints.rust]
unsafe_code = "warn"

[workspace.lints.clippy]
all = "warn"
pedantic = "warn"
nursery = "warn"
# Allow some pedantic lints that are too noisy
module_name_repetitions = "allow"
must_use_candidate = "allow"
missing_errors_doc = "allow"
missing_panics_doc = "allow"
CARGO_TOML

    log_success "Created root Cargo.toml"
}

step_6_update_host_cargo_toml() {
    log_step "Step 6: Updating crates/host/Cargo.toml"
    
    if [[ "${DRY_RUN}" == true ]]; then
        echo -e "${YELLOW}[DRY-RUN]${NC} Would update ${CRATES_DIR}/host/Cargo.toml"
        return
    fi
    
    cat > "${CRATES_DIR}/host/Cargo.toml" << 'CARGO_TOML'
[package]
name = "nvim-web-host"
description = "Neovim in the Browser - WebSocket host for browser-based Neovim"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true
homepage.workspace = true
readme = "../../README.md"
keywords = ["neovim", "browser", "websocket", "editor", "vim"]
categories = ["command-line-utilities", "development-tools"]

[dependencies]
# Workspace dependencies
anyhow.workspace = true
serde.workspace = true
rmpv.workspace = true
rmp-serde.workspace = true
serde_json.workspace = true
lazy_static.workspace = true
dirs.workspace = true
rand.workspace = true
mime_guess.workspace = true
tokio.workspace = true
futures.workspace = true
async-trait.workspace = true
tokio-tungstenite.workspace = true
tungstenite.workspace = true
axum.workspace = true
tower-http.workspace = true
http.workspace = true

# Crate-specific dependencies
ssh2 = "0.9"
rusqlite = { version = "0.31", features = ["bundled"] }
toml = "0.8"
nvim-rs = { version = "0.9", features = ["use_tokio"] }
rust-embed = { version = "8", features = ["include-exclude"] }
reqwest = { version = "0.12", features = ["json", "blocking"] }

[dev-dependencies]
tempfile = "3"

[lints]
workspace = true

[lib]
name = "nvim_web_host"
path = "src/lib.rs"

[[bin]]
name = "nvim-web-host"
path = "src/main.rs"

[[bin]]
name = "nvim-web"
path = "src/bin/cli.rs"
CARGO_TOML

    log_success "Updated crates/host/Cargo.toml"
}

step_7_update_ui_cargo_toml() {
    log_step "Step 7: Updating crates/ui/Cargo.toml"
    
    if [[ "${DRY_RUN}" == true ]]; then
        echo -e "${YELLOW}[DRY-RUN]${NC} Would update ${CRATES_DIR}/ui/Cargo.toml"
        return
    fi
    
    cat > "${CRATES_DIR}/ui/Cargo.toml" << 'CARGO_TOML'
[package]
name = "nvim-web-ui"
description = "Neovim in the Browser - WASM UI frontend"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true

[lib]
crate-type = ["cdylib"]

[dependencies]
# Workspace dependencies
wasm-bindgen.workspace = true
wasm-bindgen-futures.workspace = true
js-sys.workspace = true
rmpv.workspace = true

[dependencies.web-sys]
workspace = true
features = [
    "CanvasRenderingContext2d",
    "ClipboardEvent",
    "CloseEvent",
    "CssStyleDeclaration",
    "DataTransfer",
    "Document",
    "DomRect",
    "Element",
    "Event",
    "History",
    "HtmlElement",
    "HtmlCanvasElement",
    "Location",
    "Storage",
    "Window",
    "WebSocket",
    "MessageEvent",
    "BinaryType",
    "Blob",
    "ErrorEvent",
    "KeyboardEvent",
    "FocusEvent",
    "MouseEvent",
    "WheelEvent",
    "TouchEvent",
    "Touch",
    "TouchList",
    "EventTarget",
    "console",
    "TextMetrics",
    "ResizeObserver",
    "ResizeObserverEntry",
    "DomRectReadOnly",
    "Navigator",
    "Clipboard",
    "Headers",
    "Request",
    "RequestInit",
    "RequestMode",
    "Response",
]

[lints]
workspace = true
CARGO_TOML

    log_success "Updated crates/ui/Cargo.toml"
}

step_8_update_gitignore() {
    log_step "Step 8: Updating .gitignore for workspace"
    
    if [[ "${DRY_RUN}" == true ]]; then
        echo -e "${YELLOW}[DRY-RUN]${NC} Would update .gitignore"
        return
    fi
    
    cat > "${PROJECT_ROOT}/.gitignore" << 'GITIGNORE'
# Rust build artifacts
/target/
**/target/

# Cargo lock files in workspace members (root lock file is committed)
crates/*/Cargo.lock

# WASM build output
crates/ui/pkg/

# macOS
.DS_Store

# IDE
.idea/
.vscode/
*.swp
*.swo
*~

# Environment
.env
.env.local
config.toml

# Debug files
*.log
*.pdb

# Generated files
*.generated.*
GITIGNORE

    log_success "Updated .gitignore"
}

step_9_update_build_script() {
    log_step "Step 9: Creating workspace build script"
    
    if [[ "${DRY_RUN}" == true ]]; then
        echo -e "${YELLOW}[DRY-RUN]${NC} Would create scripts/build.sh"
        return
    fi
    
    mkdir -p "${PROJECT_ROOT}/scripts"
    
    cat > "${PROJECT_ROOT}/scripts/build.sh" << 'BUILD_SCRIPT'
#!/usr/bin/env bash
#
# Build nvim-web workspace
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

echo "Building nvim-web workspace..."

# Build host (release)
echo "Building host..."
cargo build --release -p nvim-web-host

# Build UI (WASM)
echo "Building UI (WASM)..."
cd "${PROJECT_ROOT}/crates/ui"
wasm-pack build --target web --release

echo ""
echo "Build complete!"
echo "  Host binary: target/release/nvim-web-host"
echo "  WASM pkg:    crates/ui/pkg/"
BUILD_SCRIPT

    chmod +x "${PROJECT_ROOT}/scripts/build.sh"
    log_success "Created scripts/build.sh"
}

step_10_verify_build() {
    log_step "Step 10: Verifying workspace builds"
    
    if [[ "${DRY_RUN}" == true ]]; then
        echo -e "${YELLOW}[DRY-RUN]${NC} Would run: cargo check --workspace"
        echo -e "${YELLOW}[DRY-RUN]${NC} Would run: cargo clippy --workspace"
        return
    fi
    
    cd "${PROJECT_ROOT}"
    
    log_info "Running cargo check..."
    if ! cargo check --workspace; then
        log_error "cargo check failed"
        exit 1
    fi
    log_success "cargo check passed"
    
    log_info "Running cargo clippy..."
    if ! cargo clippy --workspace -- -D warnings 2>/dev/null; then
        log_warn "cargo clippy has warnings (non-fatal)"
    else
        log_success "cargo clippy passed"
    fi
    
    log_info "Checking WASM build..."
    cd "${CRATES_DIR}/ui"
    if ! wasm-pack build --target web --dev 2>/dev/null; then
        log_error "wasm-pack build failed"
        exit 1
    fi
    log_success "wasm-pack build passed"
}

step_11_commit_changes() {
    log_step "Step 11: Committing migration"
    
    if [[ "${DRY_RUN}" == true ]]; then
        echo -e "${YELLOW}[DRY-RUN]${NC} Would commit changes"
        return
    fi
    
    cd "${PROJECT_ROOT}"
    
    git add -A
    git commit -m "refactor: migrate to Cargo workspace structure

Phase 1 of architecture restructure:
- Create crates/ directory with host and ui
- Add root Cargo.toml as virtual manifest
- Implement workspace dependency inheritance
- Move lint configs to project root
- Add workspace build script

No functional changes. Git history preserved via git mv.

Rollback: git checkout ${BACKUP_BRANCH}
"
    
    log_success "Changes committed"
}

# -----------------------------------------------------------------------------
# Main
# -----------------------------------------------------------------------------

show_help() {
    cat << EOF
nvim-web Workspace Migration Script

Usage:
    $(basename "$0") [OPTIONS]

Options:
    --dry-run    Preview changes without executing
    --help       Show this help message

Description:
    Migrates nvim-web from separate Rust projects to a unified
    Cargo workspace structure.

    Before running:
    1. Ensure you're on feature/restructure-architecture branch
    2. Ensure backup branch and tag exist
    3. Review the changes with --dry-run first

    After running:
    - Build with: cargo build --workspace
    - Test with:  cargo test --workspace
    - Rollback:   git checkout ${BACKUP_BRANCH}

EOF
}

main() {
    # Parse arguments
    while [[ $# -gt 0 ]]; do
        case "$1" in
            --dry-run)
                DRY_RUN=true
                shift
                ;;
            --help|-h)
                show_help
                exit 0
                ;;
            *)
                log_error "Unknown option: $1"
                show_help
                exit 1
                ;;
        esac
    done

    echo ""
    echo "=============================================="
    echo "  nvim-web: Cargo Workspace Migration"
    echo "  Phase 1: Foundation Setup"
    echo "=============================================="
    echo ""
    
    if [[ "${DRY_RUN}" == true ]]; then
        log_warn "DRY-RUN MODE - No changes will be made"
        echo ""
    fi

    # Run migration
    preflight_checks
    
    step_1_create_crates_dir
    step_2_move_host
    step_3_move_ui
    step_4_move_lint_configs
    step_5_create_root_cargo_toml
    step_6_update_host_cargo_toml
    step_7_update_ui_cargo_toml
    step_8_update_gitignore
    step_9_update_build_script
    step_10_verify_build
    step_11_commit_changes

    echo ""
    echo "=============================================="
    log_success "Migration complete!"
    echo "=============================================="
    echo ""
    echo "Next steps:"
    echo "  1. Review changes: git log -1 --stat"
    echo "  2. Build:          cargo build --workspace"
    echo "  3. Test:           cargo test --workspace"
    echo "  4. Push:           git push origin feature/restructure-architecture"
    echo ""
    echo "Rollback if needed:"
    echo "  git checkout ${BACKUP_BRANCH}"
    echo ""
}

main "$@"
