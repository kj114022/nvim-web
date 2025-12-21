pub mod backend;
pub mod local;
pub mod manager;

pub use backend::{VfsBackend, FileStat};
pub use local::LocalFs;
pub use manager::{VfsManager, ManagedBuffer};
