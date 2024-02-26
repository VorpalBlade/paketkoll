//! Issue describes the difference between the system and package manger

use std::path::{Path, PathBuf};

use smallvec::SmallVec;

use super::{Gid, Mode, Uid};

/// Type for vector of issues.
///
/// Optimised for almost always being empty or having at most one item.
pub type IssueVec = SmallVec<[IssueKind; 1]>;

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
#[derive(Debug)]
#[non_exhaustive]
pub enum IssueKind {
    FileMissing,
    PermissionDenied,
    TypeIncorrect,
    SizeIncorrect,
    ChecksumIncorrect,
    SymlinkTarget { actual: PathBuf, expected: PathBuf },
    WrongOwner { actual: Uid, expected: Uid },
    WrongGroup { actual: Gid, expected: Gid },
    WrongMode { actual: Mode, expected: Mode },
    MetadataError(Box<anyhow::Error>),
    FileCheckError(Box<anyhow::Error>),
}

impl std::fmt::Display for IssueKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            IssueKind::FileMissing => write!(f, "missing file")?,
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
            IssueKind::FileCheckError(err) => {
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
