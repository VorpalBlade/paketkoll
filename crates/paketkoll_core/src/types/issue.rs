//! Issue describes the difference between the system and package manger

use std::path::{Path, PathBuf};

use smallvec::SmallVec;

use super::{Gid, Mode, Uid};

/// Type for vector of issues.
///
/// Optimised for almost always being empty or having at most one item.
pub type IssueVec = SmallVec<[IssueKind; 1]>;

/// A package reference and an associated issue
pub type PackageIssue = (Option<super::PackageRef>, Issue);

/// A found difference between the file system and the package database
#[derive(Debug)]
pub struct Issue {
    path: PathBuf,
    kinds: IssueVec,
}

impl Issue {
    pub fn new(path: PathBuf, kinds: IssueVec) -> Self {
        Self { path, kinds }
    }

    /// Path of file
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Iterator over the kinds of issues
    pub fn kinds(&self) -> impl Iterator<Item = &IssueKind> {
        self.kinds.iter()
    }
}

/// Type of issue found
///
/// When the word "entity" is used below that can refer to any type
/// of file system entity (e.g. file, directory, symlink, device node, ...)
#[derive(Debug)]
#[non_exhaustive]
pub enum IssueKind {
    /// Missing entity from file system
    Missing,
    /// Extra unexpected entity on file system
    Unexpected,
    /// Failed to check for (or check contents of) entity due to permissions
    PermissionDenied,
    /// Type of entity was not as expected (e.g. file vs symlink)
    TypeIncorrect,
    /// The file was not of the expected size
    SizeIncorrect,
    /// The contents of the file differ (different checksums)
    ChecksumIncorrect,
    /// Both entity are symlinks, but point to different places
    SymlinkTarget { actual: PathBuf, expected: PathBuf },
    /// Ownership of file system entity differs
    WrongOwner { actual: Uid, expected: Uid },
    /// Group of file system entity differs
    WrongGroup { actual: Gid, expected: Gid },
    /// Incorrect mode for file system entity
    WrongMode { actual: Mode, expected: Mode },
    /// Some sort of parsing error for this entry (from the package manager backend)
    MetadataError(Box<anyhow::Error>),
    /// Some sort of unexpected error when processing the file system
    FsCheckError(Box<anyhow::Error>),
}

impl std::fmt::Display for IssueKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IssueKind::Missing => write!(f, "missing or inaccessible file/directory/...")?,
            IssueKind::Unexpected => write!(f, "unexpected file")?,
            IssueKind::PermissionDenied => write!(f, "read error (Permission denied)")?,
            IssueKind::TypeIncorrect => write!(f, "type mismatch")?,
            IssueKind::SizeIncorrect => write!(f, "size mismatch")?,
            IssueKind::ChecksumIncorrect => write!(f, "checksum mismatch")?,
            IssueKind::SymlinkTarget { actual, expected } => write!(
                f,
                "symlink target mismatch (expected {expected:?}, actual {actual:?})"
            )?,
            IssueKind::WrongOwner { actual, expected } => {
                write!(f, "UID mismatch (expected {expected}, actual {actual})")?
            }
            IssueKind::WrongGroup { actual, expected } => {
                write!(f, "GID mismatch (expected {expected}, actual {actual})")?
            }
            IssueKind::WrongMode { actual, expected } => write!(
                f,
                "permission mismatch (expected {expected}, actual {actual})"
            )?,
            IssueKind::MetadataError(err) => {
                write!(f, "error with metadata parsing")?;
                format_error(f, err)?;
            }
            IssueKind::FsCheckError(err) => {
                write!(f, "error when checking file")?;
                format_error(f, err)?;
            }
        }
        Ok(())
    }
}

/// Trying to get useful formatting for errors is a mess on stable Rust
/// (it's better on nightly, but we don't want to require that).
/// Especially backtraces are missing.
fn format_error(f: &mut std::fmt::Formatter<'_>, err: &anyhow::Error) -> std::fmt::Result {
    for cause in err.chain() {
        write!(f, "\n   Caused by: {}", cause)?;
    }
    if Ok("1".into()) == std::env::var("RUST_BACKTRACE") {
        write!(f, "\n   Backtrace: {}", err.backtrace())?;
    }
    Ok(())
}
