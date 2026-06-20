pub mod config;
pub mod error;
pub mod flags;
pub mod lockfile;
pub mod package;
pub mod registry;

// Heavy implementation modules — to be filled by agents
pub mod builder;
pub mod fetcher;
pub mod include;
pub mod resolver;

pub use config::HutConfig;
pub use error::{HutError, HutResult};
pub use lockfile::Lockfile;
pub use package::Package;
pub use registry::RegistryIndex;
