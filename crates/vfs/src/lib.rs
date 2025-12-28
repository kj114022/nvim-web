pub mod backend;
pub mod browser;
pub mod git;
pub mod http;
pub mod local;
pub mod manager;
pub mod ssh;

pub use backend::{FileStat, VfsBackend};
pub use browser::{BrowserFsBackend, FsRequestRegistry};
pub use git::GitFsBackend;
pub use http::HttpFsBackend;
pub use local::LocalFs;
pub use manager::{ManagedBuffer, VfsManager};
pub use ssh::SshFsBackend;

