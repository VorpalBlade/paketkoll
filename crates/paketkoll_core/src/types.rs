//! Shared types

mod files;
mod issue;
mod package;

pub(crate) use files::{
    Checksum, Directory, FileEntry, FileFlags, Gid, Mode, Properties, RegularFile,
    RegularFileBasic, Symlink, Uid,
};
pub use issue::{Issue, IssueKind, IssueVec, PackageIssue};

pub use package::InstallReason;
pub use package::Package;
pub use package::PackageInterner;
pub use package::PackageRef;
