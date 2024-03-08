//! Common types for representing data about files

use std::{path::PathBuf, sync::atomic::AtomicBool, time::SystemTime};

use super::PackageRef;

/// A regular file with just checksum info (as Debian gives us)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RegularFileBasic {
    pub checksum: Checksum,
}

/// A regular file with all info (as Arch Linux has)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct RegularFile {
    pub mode: Mode,
    pub owner: Uid,
    pub group: Gid,
    pub size: u64,
    pub mtime: SystemTime,
    pub checksum: Checksum,
}

/// A symlink
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Symlink {
    pub owner: Uid,
    pub group: Gid,
    /// Note: May be a relative path
    pub target: PathBuf,
}

/// A directory
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct Directory {
    pub mode: Mode,
    pub owner: Uid,
    pub group: Gid,
}

/// Handles weird cases we don't support (device nodes for example).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Special;

/// If the package management system doesn't give us enough info,
/// all we know is that it should exist.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Unknown;

/// A file entry from the package database
#[derive(Debug)]
#[non_exhaustive]
pub(crate) struct FileEntry {
    /// Package this file belongs to
    pub package: Option<PackageRef>,
    pub path: PathBuf,
    pub properties: Properties,
    pub flags: FileFlags,
    /// Used to handle finding missing files when checking for unexpected files
    pub(crate) seen: AtomicBool,
    // Has one byte of padding left over for something else
}

impl Clone for FileEntry {
    fn clone(&self) -> Self {
        Self {
            package: self.package,
            path: self.path.clone(),
            properties: self.properties.clone(),
            flags: self.flags,
            seen: self.seen.load(std::sync::atomic::Ordering::Relaxed).into(),
        }
    }
}

impl PartialEq for FileEntry {
    fn eq(&self, other: &Self) -> bool {
        self.package == other.package
            && self.path == other.path
            && self.properties == other.properties
            && self.flags == other.flags
    }
}

bitflags::bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub(crate) struct FileFlags : u16 {
        const CONFIG = 0b0000_0000_0000_0001;
    }
}

/// File properties from the package database
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Properties {
    /// A regular file with just checksum info (as Debian gives us)
    RegularFileBasic(RegularFileBasic),
    /// A regular file with all info (as Arch Linux has)
    RegularFile(RegularFile),
    Symlink(Symlink),
    Directory(Directory),
    /// Handles weird cases we don't support (device nodes for example).
    Special,
    /// If the package management system doesn't give us enough info,
    /// all we know is that it should exist.
    Unknown,
}

impl Properties {
    pub(crate) fn is_dir(&self) -> Option<bool> {
        match self {
            Properties::RegularFileBasic(_) => Some(false),
            Properties::RegularFile(_) => Some(false),
            Properties::Symlink(_) => Some(false),
            Properties::Directory(_) => Some(true),
            Properties::Special => Some(false),
            Properties::Unknown => None,
        }
    }
}

/// Unix file mode (permissions)
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Mode(pub u32);

impl Mode {
    pub fn new(value: u32) -> Self {
        Self(value)
    }
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:o}", self.0)
    }
}

/// A POSIX UID
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Uid(pub u32);

impl Uid {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for Uid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

/// A POSIX GID
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct Gid(pub u32);

impl Gid {
    pub fn new(id: u32) -> Self {
        Self(id)
    }
}

impl std::fmt::Display for Gid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) enum Checksum {
    #[cfg(feature = "__md5")]
    Md5([u8; 16]),
    #[cfg(feature = "__sha256")]
    Sha256([u8; 32]),
}
