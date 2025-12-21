pub mod backend;
pub mod local;
pub mod browser;
pub mod manager;

pub use backend::{VfsBackend, FileStat};
pub use local::LocalFs;
pub use browser::BrowserFsBackend;
pub use manager::{VfsManager, ManagedBuffer};
