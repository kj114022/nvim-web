#!/usr/bin/env bash
#
# nvim-web: Phase 2 - VFS Extraction
# Moves VFS logic from host to crates/vfs
#

set -euo pipefail
IFS=$'\n\t'

# -----------------------------------------------------------------------------
# Configuration
# -----------------------------------------------------------------------------

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
readonly HOSTE_VFS_DIR="${PROJECT_ROOT}/crates/host/src/vfs"
readonly NEW_VFS_DIR="${PROJECT_ROOT}/crates/vfs"

# Colors
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly BLUE='\033[0;34m'
readonly RED='\033[0;31m'
readonly NC='\033[0m'

DRY_RUN=false

# -----------------------------------------------------------------------------
# Logging
# -----------------------------------------------------------------------------

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_success() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_step() { echo -e "\n${GREEN}==>${NC} ${BLUE}$1${NC}"; }

# -----------------------------------------------------------------------------
# Steps
# -----------------------------------------------------------------------------

step_1_create_vfs_crate() {
    log_step "Step 1: Creating crates/vfs"
    
    if [[ "${DRY_RUN}" == true ]]; then
        log_warn "[DRY-RUN] Would create ${NEW_VFS_DIR}"
        return
    fi
    
    mkdir -p "${NEW_VFS_DIR}/src"
    
    # Create Cargo.toml
    cat > "${NEW_VFS_DIR}/Cargo.toml" << 'TOML'
[package]
name = "nvim-web-vfs"
description = "Virtual Filesystem layer for nvim-web"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
# Dependencies extracted from host/src/vfs usage
anyhow.workspace = true
async-trait.workspace = true
tokio.workspace = true
dirs.workspace = true
ssh2 = "0.9" # Used by ssh backend
std-path = "0.1" 

[lints]
workspace = true
TOML
    
    log_success "Created crates/vfs/Cargo.toml"
}

step_2_move_vfs_sources() {
    log_step "Step 2: Moving source files from host"
    
    if [[ "${DRY_RUN}" == true ]]; then
        log_warn "[DRY-RUN] Would move files from ${HOSTE_VFS_DIR}"
        return
    fi
    
    # Move files using git mv to preserve history
    # Files: backend.rs, browser.rs, local.rs, manager.rs, mod.rs, ssh.rs
    
    # We move mod.rs to lib.rs
    if [[ -f "${HOSTE_VFS_DIR}/mod.rs" ]]; then
        git mv "${HOSTE_VFS_DIR}/mod.rs" "${NEW_VFS_DIR}/src/lib.rs"
        log_success "Moved mod.rs -> lib.rs"
    fi
    
    for file in backend.rs browser.rs local.rs manager.rs ssh.rs; do
         if [[ -f "${HOSTE_VFS_DIR}/${file}" ]]; then
            git mv "${HOSTE_VFS_DIR}/${file}" "${NEW_VFS_DIR}/src/${file}"
            log_success "Moved ${file}"
        fi
    done
    
    # Remove old directory if empty
    rmdir "${HOSTE_VFS_DIR}" 2>/dev/null || true
}

step_3_update_host_imports() {
    log_step "Step 3: Updating host Cargo.toml"
    
    if [[ "${DRY_RUN}" == true ]]; then
        log_warn "[DRY-RUN] Would update host Cargo.toml"
        return
    fi
    
    # Add dependency
    # Note: Use sed carefully. Better to append to [dependencies] if exists, 
    # but we already have other workspace deps.
    
     if ! grep -q "nvim-web-vfs" "${PROJECT_ROOT}/crates/host/Cargo.toml"; then
          # Append to dependencies section (simple logic: append at end of file if [dependencies] exists logic fails)
          # Pratically, just appending to end is risky if [[bin]] sections exist at end.
          # We'll use a safer insertion using perl/sed searching for a known dependency.
          
          # Insert after nvim-web-protocol line
          sed -i '' '/nvim-web-protocol/a\
nvim-web-vfs = { path = "../vfs" }
' "${PROJECT_ROOT}/crates/host/Cargo.toml"
          log_success "Added nvim-web-vfs dependency to host"
     fi
}

step_4_register_workspace() {
     log_step "Step 4: Registering vfs in workspace"

    if [[ "${DRY_RUN}" == true ]]; then
        log_warn "[DRY-RUN] Would update root Cargo.toml"
        return
    fi
    
    if ! grep -q "crates/vfs" "${PROJECT_ROOT}/Cargo.toml"; then
        sed -i '' '/members = \[/a\
    "crates/vfs",
' "${PROJECT_ROOT}/Cargo.toml"
        log_success "Added definition to workspace members"
    fi
}


step_5_verify() {
    log_step "Step 5: Verification"
    
    if [[ "${DRY_RUN}" == true ]]; then
        log_warn "[DRY-RUN] Would run cargo check"
        return
    fi
    
    log_info "Running cargo check --workspace..."
    # This might fail initially because host code still imports crate::vfs
    # We will need manual fixups for the imports in the codebase
    
    if cargo check --workspace; then
        log_success "Build passed!"
    else
        log_warn "Build failed (expected). Host code imports need manual update from 'crate::vfs' to 'nvim_web_vfs'."
    fi
}

main() {
     if [[ "${1:-}" == "--dry-run" ]]; then
        DRY_RUN=true
        log_warn "DRY RUN MODE"
    fi
    
    step_1_create_vfs_crate
    step_2_move_vfs_sources
    step_3_update_host_imports
    step_4_register_workspace
    step_5_verify
    
    log_success "Phase 2 VFS Extraction - File Move Complete"
    log_info "Next: manually find/replace 'crate::vfs' with 'nvim_web_vfs' in host crate."
}

main "$@"
