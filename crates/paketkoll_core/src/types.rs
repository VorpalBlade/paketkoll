//! Shared types

mod files;
mod package;

pub(crate) use files::{
    DeviceNode, DeviceType, Directory, Fifo, FileEntry, FileFlags, Permissions, Properties,
    RegularFile, RegularFileBasic, RegularFileSystemd, Symlink,
};

pub(crate) use package::BackendData;
pub use package::Dependency;
pub use package::InstallReason;
pub use package::Package;
pub(crate) use package::PackageBuilder;
pub use package::PackageDirect;
pub use package::PackageInstallStatus;
pub use package::PackageInterned;

// Re-export types from paketkoll_types
pub use paketkoll_types::files::Gid;
pub use paketkoll_types::files::Mode;
pub use paketkoll_types::files::Uid;
pub use paketkoll_types::intern::ArchitectureRef;
pub use paketkoll_types::intern::Interner;
pub use paketkoll_types::intern::PackageRef;
pub use paketkoll_types::issue::{EntryType, Issue, IssueKind, IssueVec, PackageIssue};
