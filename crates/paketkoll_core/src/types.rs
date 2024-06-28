//! Shared types

mod files;
mod issue;
mod package;

pub(crate) use files::{
    Checksum, DeviceNode, DeviceType, Directory, Fifo, FileEntry, FileFlags, Permissions,
    Properties, RegularFile, RegularFileBasic, RegularFileSystemd, Symlink,
};
pub use files::{Gid, Mode, Uid};
pub use issue::{EntryType, Issue, IssueKind, IssueVec, PackageIssue};

pub use package::ArchitectureRef;
pub(crate) use package::BackendData;
pub use package::Dependency;
pub use package::InstallReason;
pub use package::Interner;
pub use package::Package;
pub(crate) use package::PackageBuilder;
pub use package::PackageDirect;
pub use package::PackageInstallStatus;
pub use package::PackageInterned;
pub use package::PackageRef;
