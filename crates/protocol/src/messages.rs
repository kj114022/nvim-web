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
