//! Shared types

mod files;
mod issue;
mod package;

pub(crate) use files::{
    Checksum, Directory, FileEntry, FileFlags, Gid, Mode, Properties, RegularFile,
    RegularFileBasic, Symlink, Uid,
};
pub(crate) use issue::{Issue, IssueKind, IssueVec};

pub(crate) use package::InstallReason;
pub use package::PackageInterner;
pub use package::PackageRef;
// Not yet ready to make public
pub(crate) use package::Package;
