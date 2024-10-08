//! Issue describes the difference between the system and package manager

use crate::files::Checksum;
use crate::files::DeviceType;
use crate::files::Gid;
use crate::files::Mode;
use crate::files::Uid;
use crate::intern::PackageRef;
use smallvec::SmallVec;
use std::fmt::Display;
use std::os::unix::fs::FileTypeExt;
use std::path::Path;
use std::path::PathBuf;

/// Type for vector of issues.
///
/// Optimised for almost always being empty or having at most one item.
pub type IssueVec = SmallVec<[IssueKind; 1]>;

/// A package reference and an associated issue
pub type PackageIssue = (Option<PackageRef>, Issue);

/// Type of entry (used to report mismatches)
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EntryType {
    RegularFile,
    Directory,
    Symlink,
    BlockDevice,
    CharDevice,
    Fifo,
    Socket,
    /// Anything except file, directory or symlink
    Special,
    /// Anything except a normal file (Debian really doesn't report much info)
    Unknown,
}

impl From<std::fs::FileType> for EntryType {
    fn from(value: std::fs::FileType) -> Self {
        if value.is_dir() {
            Self::Directory
        } else if value.is_file() {
            Self::RegularFile
        } else if value.is_symlink() {
            Self::Symlink
        } else if value.is_block_device() {
            Self::BlockDevice
        } else if value.is_char_device() {
            Self::CharDevice
        } else if value.is_fifo() {
            Self::Fifo
        } else if value.is_socket() {
            Self::Socket
        } else {
            panic!("Unknown file type {value:?}")
        }
    }
}

impl Display for EntryType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::RegularFile => write!(f, "file"),
            Self::Directory => write!(f, "directory"),
            Self::Symlink => write!(f, "symlink"),
            Self::BlockDevice => write!(f, "block device"),
            Self::CharDevice => write!(f, "character device"),
            Self::Fifo => write!(f, "FIFO"),
            Self::Socket => write!(f, "socket"),
            Self::Special => write!(f, "special file"),
            Self::Unknown => write!(f, "unknown non-regular file"),
        }
    }
}

/// A found difference between the file system and the package database
#[derive(Debug, serde::Serialize)]
pub struct Issue {
    path: PathBuf,
    kinds: IssueVec,
    source: Option<&'static str>,
}

impl Issue {
    #[must_use]
    pub const fn new(path: PathBuf, kinds: IssueVec, source: Option<&'static str>) -> Self {
        Self {
            path,
            kinds,
            source,
        }
    }

    /// Path of file
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Iterator over the kinds of issues
    pub fn kinds(&self) -> impl Iterator<Item = &IssueKind> {
        self.kinds.iter()
    }

    #[must_use]
    pub const fn source(&self) -> Option<&'static str> {
        self.source
    }
}

/// Type of issue found
///
/// When the word "entity" is used below that can refer to any type
/// of file system entity (e.g. file, directory, symlink, device node, ...)
#[derive(Debug)]
#[non_exhaustive]
#[derive(serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum IssueKind {
    /// Missing entity from file system
    Missing,
    /// Entry on file system exists, but shouldn't (it is being actively
    /// removed)
    Exists,
    /// Extra unexpected entity on file system
    Unexpected,
    /// Failed to check for (or check contents of) entity due to permissions
    PermissionDenied,
    /// Type of entity was not as expected (e.g. file vs symlink)
    TypeIncorrect {
        actual: EntryType,
        expected: EntryType,
    },
    /// The file was not of the expected size
    SizeIncorrect { actual: u64, expected: u64 },
    /// The contents of the file differ (different checksums)
    ChecksumIncorrect {
        actual: Checksum,
        expected: Checksum,
    },
    /// Both entity are symlinks, but point to different places
    SymlinkTarget { actual: PathBuf, expected: PathBuf },
    /// Ownership of file system entity differs
    WrongOwner { actual: Uid, expected: Uid },
    /// Group of file system entity differs
    WrongGroup { actual: Gid, expected: Gid },
    /// Incorrect mode for file system entity
    WrongMode { actual: Mode, expected: Mode },
    /// Incorrect major or minor device node numbers
    WrongDeviceNodeId {
        actual: (DeviceType, u64, u64),
        expected: (DeviceType, u64, u64),
    },
    /// Some sort of parsing error for this entry (from the package manager
    /// backend)
    #[serde(serialize_with = "serialize_error")]
    MetadataError(Box<eyre::Error>),
    /// Some sort of unexpected error when processing the file system
    #[serde(serialize_with = "serialize_error")]
    FsCheckError(Box<eyre::Error>),
}

impl Display for IssueKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Missing => write!(f, "missing or inaccessible file/directory/...")?,
            Self::Exists => write!(f, "unexpected file/directory/... (should be removed)")?,
            Self::Unexpected => write!(f, "unexpected file")?,
            Self::PermissionDenied => write!(f, "read error (Permission denied)")?,
            Self::TypeIncorrect { actual, expected } => {
                write!(f, "type mismatch (expected {expected}, actual {actual})")?;
            }
            Self::SizeIncorrect { actual, expected } => {
                write!(f, "size mismatch (expected {expected}, actual {actual})")?;
            }
            Self::ChecksumIncorrect { actual, expected } => write!(
                f,
                "checksum mismatch (expected {expected}, actual {actual})"
            )?,
            Self::SymlinkTarget { actual, expected } => write!(
                f,
                "symlink target mismatch (expected {expected:?}, actual {actual:?})"
            )?,
            Self::WrongOwner { actual, expected } => {
                write!(f, "UID mismatch (expected {expected}, actual {actual})")?;
            }
            Self::WrongGroup { actual, expected } => {
                write!(f, "GID mismatch (expected {expected}, actual {actual})")?;
            }
            Self::WrongMode { actual, expected } => write!(
                f,
                "permission mismatch (expected {expected}, actual {actual})"
            )?,
            Self::WrongDeviceNodeId { actual, expected } => write!(
                f,
                "device node ID mismatch (expected {} {}:{}, actual {} {}:{})",
                expected.0, expected.1, expected.2, actual.0, actual.1, actual.2,
            )?,
            Self::MetadataError(err) => {
                write!(f, "error with metadata parsing")?;
                format_error(f, err)?;
            }
            Self::FsCheckError(err) => {
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
fn format_error(f: &mut std::fmt::Formatter<'_>, err: &eyre::Error) -> std::fmt::Result {
    for cause in err.chain() {
        write!(f, "\n   Caused by: {cause}")?;
    }
    //if Ok("1".into()) == std::env::var("RUST_BACKTRACE") {
    //    write!(f, "\n   Backtrace: {}", err.backtrace())?;
    //}
    Ok(())
}

fn serialize_error<S>(err: &eyre::Error, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_str(&format!("{err}"))
}
