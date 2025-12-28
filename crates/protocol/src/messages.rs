

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

impl std::fmt::Display for InternalMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::VfsOpen => "vfs_open",
            Self::VfsWrite => "vfs_write",
            Self::VfsList => "vfs_list",
            Self::SettingsGet => "settings_get",
            Self::SettingsSet => "settings_set",
            Self::SettingsAll => "settings_all",
            Self::GetCwdInfo => "get_cwd_info",
            Self::ClipboardReadResponse => "clipboard_read_response",
            Self::Unknown(s) => s,
        };
        write!(f, "{s}")
    }
}
