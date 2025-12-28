#!/usr/bin/env bash
#
# nvim-web: Phase 2 - Protocol Extraction
# Extracts shared types to crates/protocol
#

set -euo pipefail
IFS=$'\n\t'

# -----------------------------------------------------------------------------
# Configuration
# -----------------------------------------------------------------------------

readonly SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
readonly PROJECT_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"
readonly CRATES_DIR="${PROJECT_ROOT}/crates"
readonly PROTOCOL_DIR="${CRATES_DIR}/protocol"

# Colors
readonly GREEN='\033[0;32m'
readonly YELLOW='\033[1;33m'
readonly BLUE='\033[0;34m'
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

step_1_create_protocol_crate() {
    log_step "Step 1: Creating crates/protocol"
    
    if [[ "${DRY_RUN}" == true ]]; then
        log_warn "[DRY-RUN] Would create ${PROTOCOL_DIR}"
        return
    fi
    
    mkdir -p "${PROTOCOL_DIR}/src"
    
    # Create Cargo.toml
    cat > "${PROTOCOL_DIR}/Cargo.toml" << 'TOML'
[package]
name = "nvim-web-protocol"
description = "Shared protocol types for nvim-web"
version.workspace = true
edition.workspace = true
license.workspace = true
repository.workspace = true

[dependencies]
serde.workspace = true
rmpv.workspace = true
rmp-serde.workspace = true
serde_json.workspace = true
tokio-tungstenite.workspace = true  # Re-export Message type
tungstenite.workspace = true

[lints]
workspace = true
TOML
    
    log_success "Created crates/protocol/Cargo.toml"
}

step_2_create_protocol_files() {
    log_step "Step 2: Defining shared protocol types"
    
    if [[ "${DRY_RUN}" == true ]]; then
        log_warn "[DRY-RUN] Would create source files in ${PROTOCOL_DIR}/src/"
        return
    fi
    
    # src/lib.rs
    cat > "${PROTOCOL_DIR}/src/lib.rs" << 'RUST'
//! Shared protocol types for nvim-web
//! 
//! Defines the MessagePack-RPC structures used between Host and UI.

pub mod messages;
pub mod rpc;

pub use messages::*;
pub use rpc::*;
RUST

    # src/rpc.rs
    cat > "${PROTOCOL_DIR}/src/rpc.rs" << 'RUST'
use serde::{Deserialize, Serialize};
use rmpv::Value;

/// MessagePack-RPC Message Type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RpcMessageType {
    Request = 0,
    Response = 1,
    Notification = 2,
}

/// Helper to parse RPC message type
pub fn parse_message_type(val: &Value) -> Option<RpcMessageType> {
    match val.as_i64() {
        Some(0) => Some(RpcMessageType::Request),
        Some(1) => Some(RpcMessageType::Response),
        Some(2) => Some(RpcMessageType::Notification),
        _ => None,
    }
}
RUST

    # src/messages.rs
    cat > "${PROTOCOL_DIR}/src/messages.rs" << 'RUST'
use serde::{Deserialize, Serialize};

/// Known internal RPC methods
pub enum InternalMethod {
    // VFS Operations
    VfsOpen,  // vfs_open
    VfsWrite, // vfs_write
    VfsList,  // vfs_list
    
    // Settings
    SettingsGet, // settings_get
    SettingsSet, // settings_set
    SettingsAll, // settings_all
    
    // Status
    GetCwdInfo, // get_cwd_info
    
    // Clipboard
    ClipboardReadResponse, // clipboard_read_response
    
    Unknown(String),
}

impl From<&str> for InternalMethod {
    fn from(s: &str) -> Self {
        match s {
            "vfs_open" => Self::VfsOpen,
            "vfs_write" => Self::VfsWrite,
            "vfs_list" => Self::VfsList,
            "settings_get" => Self::SettingsGet,
            "settings_set" => Self::SettingsSet,
            "settings_all" => Self::SettingsAll,
            "get_cwd_info" => Self::GetCwdInfo,
            "clipboard_read_response" => Self::ClipboardReadResponse,
            other => Self::Unknown(other.to_string()),
        }
    }
}

impl ToString for InternalMethod {
    fn to_string(&self) -> String {
        match self {
            Self::VfsOpen => "vfs_open".to_string(),
            Self::VfsWrite => "vfs_write".to_string(),
            Self::VfsList => "vfs_list".to_string(),
            Self::SettingsGet => "settings_get".to_string(),
            Self::SettingsSet => "settings_set".to_string(),
            Self::SettingsAll => "settings_all".to_string(),
            Self::GetCwdInfo => "get_cwd_info".to_string(),
            Self::ClipboardReadResponse => "clipboard_read_response".to_string(),
            Self::Unknown(s) => s.clone(),
        }
    }
}
RUST
    
    log_success "Created protocol source files"
}

step_3_register_in_workspace() {
    log_step "Step 3: Registering protocol crate in workspace"
    
    if [[ "${DRY_RUN}" == true ]]; then
        log_warn "[DRY-RUN] Would update root Cargo.toml"
        return
    fi
    
    # Check if already registered
    if grep -q "crates/protocol" "${PROJECT_ROOT}/Cargo.toml"; then
        log_info "Protocol crate already in workspace members"
    else
        # Insert "crates/protocol", into members [ ... ]
        # This is a bit fragile with sed, simpler to append or manual
        # For script robustness, assuming standard format
        sed -i '' '/members = \[/a\
    "crates/protocol",
' "${PROJECT_ROOT}/Cargo.toml"
        log_success "Added definition to workspace members"
    fi
}

step_4_update_dependencies() {
    log_step "Step 4: Adding protocol dependency to host/ui"
    
    if [[ "${DRY_RUN}" == true ]]; then
        log_warn "[DRY-RUN] Would update host/ui Cargo.toml"
        return
    fi
    
    # Add to host
    if ! grep -q "nvim-web-protocol" "${CRATES_DIR}/host/Cargo.toml"; then
        echo -e "\nnvim-web-protocol = { path = \"../protocol\" }" >> "${CRATES_DIR}/host/Cargo.toml"
        log_success "Added dependency to host"
    fi
    
    # Add to ui
    if ! grep -q "nvim-web-protocol" "${CRATES_DIR}/ui/Cargo.toml"; then
        echo -e "\nnvim-web-protocol = { path = \"../protocol\" }" >> "${CRATES_DIR}/ui/Cargo.toml"
        log_success "Added dependency to ui"
    fi
}

step_5_verify() {
    log_step "Step 5: Verification"
    
    if [[ "${DRY_RUN}" == true ]]; then
        log_warn "[DRY-RUN] Would run cargo check"
        return
    fi
    
    log_info "Running cargo check --workspace..."
    cargo check --workspace
}

# -----------------------------------------------------------------------------
# Main
# -----------------------------------------------------------------------------

main() {
    if [[ "${1:-}" == "--dry-run" ]]; then
        DRY_RUN=true
        log_warn "DRY RUN MODE"
    fi
    
    step_1_create_protocol_crate
    step_2_create_protocol_files
    step_3_register_in_workspace
    step_4_update_dependencies
    step_5_verify
    
    log_success "Phase 2 Extraction Complete!"
}

main "$@"
