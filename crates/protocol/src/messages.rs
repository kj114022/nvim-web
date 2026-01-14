/// Known internal RPC methods
pub enum InternalMethod {
    // VFS Operations
    VfsOpen,      // vfs_open
    VfsWrite,     // vfs_write
    VfsList,      // vfs_list
    VfsReadChunk, // vfs_read_chunk
    VfsFileInfo,  // vfs_file_info

    // Settings
    SettingsGet, // settings_get
    SettingsSet, // settings_set
    SettingsAll, // settings_all

    // Status
    GetCwdInfo, // get_cwd_info

    // Clipboard
    ClipboardReadResponse, // clipboard_read_response

    // LLM Operations
    LlmPrompt,      // llm_prompt
    LlmCancel,      // llm_cancel
    LlmListModels,  // llm_list_models
    LlmSetKey,      // llm_set_key
    LlmSetProvider, // llm_set_provider

    // Search
    Search, // search

    Unknown(String),
}

impl From<&str> for InternalMethod {
    fn from(s: &str) -> Self {
        match s {
            "vfs_open" => Self::VfsOpen,
            "vfs_write" => Self::VfsWrite,
            "vfs_list" => Self::VfsList,
            "vfs_read_chunk" => Self::VfsReadChunk,
            "vfs_file_info" => Self::VfsFileInfo,
            "settings_get" => Self::SettingsGet,
            "settings_set" => Self::SettingsSet,
            "settings_all" => Self::SettingsAll,
            "get_cwd_info" => Self::GetCwdInfo,
            "clipboard_read_response" => Self::ClipboardReadResponse,
            "llm_prompt" => Self::LlmPrompt,
            "llm_cancel" => Self::LlmCancel,
            "llm_list_models" => Self::LlmListModels,
            "llm_set_key" => Self::LlmSetKey,
            "llm_set_provider" => Self::LlmSetProvider,
            "search" => Self::Search,
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
            Self::VfsReadChunk => "vfs_read_chunk",
            Self::VfsFileInfo => "vfs_file_info",
            Self::SettingsGet => "settings_get",
            Self::SettingsSet => "settings_set",
            Self::SettingsAll => "settings_all",
            Self::GetCwdInfo => "get_cwd_info",
            Self::ClipboardReadResponse => "clipboard_read_response",
            Self::LlmPrompt => "llm_prompt",
            Self::LlmCancel => "llm_cancel",
            Self::LlmListModels => "llm_list_models",
            Self::LlmSetKey => "llm_set_key",
            Self::LlmSetProvider => "llm_set_provider",
            Self::Search => "search",
            Self::Unknown(s) => s,
        };
        write!(f, "{s}")
    }
}
