//! A parser for systemd-tmpfiles configuration files

mod architecture;
pub mod parser;
pub mod specifier;
mod types;

pub use types::Age;
pub use types::DeviceNode;
pub use types::Directive;
pub use types::Entry;
pub use types::EntryFlags;
pub use types::Id;
pub use types::Mode;
pub use types::SubvolumeQuota;
