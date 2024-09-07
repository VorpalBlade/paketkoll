//! Types representing things about the file system

use crate::intern::PackageRef;
use std::fmt::Octal;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

/// Unix file mode (permissions)
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[repr(transparent)]
pub struct Mode(u32);

impl Mode {
    #[inline]
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    #[inline]
    #[must_use]
    pub const fn as_raw(&self) -> u32 {
        self.0
    }

    /// Parse from u=rwx,g=rw,o=r format
    ///
    /// Since that is a diff
    pub fn parse(value: &str) -> eyre::Result<Self> {
        let components = value.split(',');

        let mut mode: u32 = 0;
        for component in components {
            let component = component.as_bytes();
            let who = component
                .first()
                .ok_or_else(|| eyre::eyre!("Invalid mode: {value}"))?;
            if component[1] != b'=' {
                return Err(eyre::eyre!("Invalid mode: {value}"));
            }
            let perms = &component[2..];

            // Convert perms to bits
            let mut bits = 0;
            for perm in perms {
                match perm {
                    b'r' => bits |= 0b100,
                    b'w' => bits |= 0b010,
                    b'x' => bits |= 0b001,
                    _ => return Err(eyre::eyre!("Invalid mode: {value}")),
                }
            }

            // Shift the bits based on who
            match who {
                b'u' => mode |= bits << 6,
                b'g' => mode |= bits << 3,
                b'o' => mode |= bits,
                _ => return Err(eyre::eyre!("Invalid mode: {value}")),
            }
        }
        Ok(Self(mode))
    }
}

impl std::fmt::Display for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:o}", self.0)
    }
}

impl Octal for Mode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:o}", self.0)
    }
}

impl From<Mode> for nix::sys::stat::Mode {
    fn from(mode: Mode) -> Self {
        Self::from_bits_truncate(mode.0)
    }
}

/// A POSIX UID
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[repr(transparent)]
pub struct Uid(u32);

impl Uid {
    #[inline]
    #[must_use]
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    #[inline]
    #[must_use]
    pub fn as_raw(&self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for Uid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

impl From<Uid> for nix::unistd::Uid {
    fn from(uid: Uid) -> Self {
        Self::from_raw(uid.0)
    }
}

impl From<&Uid> for nix::unistd::Uid {
    fn from(uid: &Uid) -> Self {
        Self::from_raw(uid.0)
    }
}

/// A POSIX GID
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[repr(transparent)]
pub struct Gid(u32);

impl Gid {
    #[inline]
    #[must_use]
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    #[inline]
    #[must_use]
    pub fn as_raw(&self) -> u32 {
        self.0
    }
}

impl From<Gid> for nix::unistd::Gid {
    fn from(gid: Gid) -> Self {
        Self::from_raw(gid.0)
    }
}

impl From<&Gid> for nix::unistd::Gid {
    fn from(gid: &Gid) -> Self {
        Self::from_raw(gid.0)
    }
}

impl std::fmt::Display for Gid {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Display::fmt(&self.0, f)
    }
}

/// Represents a checksum of a file
///
/// Which checksum types are used depend on the feature flags.
/// For example currently: Arch uses SHA256, and Debian uses MD5.
#[derive(
    Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum Checksum {
    #[serde(with = "serde_bytes")]
    Md5([u8; 16]),
    #[serde(with = "serde_bytes")]
    Sha256([u8; 32]),
}

impl std::fmt::Display for Checksum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Md5(value) => write!(f, "md5:{}", faster_hex::hex_string(value)),
            Self::Sha256(value) => write!(f, "sha256:{}", faster_hex::hex_string(value)),
        }
    }
}

/// A regular file with just checksum info (as Debian gives us)
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RegularFileBasic {
    pub size: Option<u64>,
    pub checksum: Checksum,
}

/// A regular file with all info (as Arch Linux has)
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RegularFile {
    pub mode: Mode,
    pub owner: Uid,
    pub group: Gid,
    pub size: u64,
    pub mtime: SystemTime,
    pub checksum: Checksum,
}

/// A regular file with all info (as Arch Linux has)
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct RegularFileSystemd {
    pub mode: Mode,
    pub owner: Uid,
    pub group: Gid,
    pub size: Option<u64>,
    pub checksum: Checksum,
    pub contents: Option<Box<[u8]>>,
}

/// A FIFO
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Fifo {
    pub mode: Mode,
    pub owner: Uid,
    pub group: Gid,
}

/// A device node
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct DeviceNode {
    pub mode: Mode,
    pub owner: Uid,
    pub group: Gid,
    pub device_type: DeviceType,
    pub major: u64,
    pub minor: u64,
}

/// Type of device node (block or char)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum DeviceType {
    Block,
    Char,
}

impl std::fmt::Display for DeviceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Block => write!(f, "block"),
            Self::Char => write!(f, "char"),
        }
    }
}

/// A symlink
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Symlink {
    pub owner: Uid,
    pub group: Gid,
    /// Note: May be a relative path
    pub target: PathBuf,
}

/// A directory
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Directory {
    pub mode: Mode,
    pub owner: Uid,
    pub group: Gid,
}

/// A mapping from paths to file entries
pub type PathMap<'a> = ahash::AHashMap<&'a Path, &'a FileEntry>;

/// A file entry from the package database
#[derive(Debug)]
pub struct FileEntry {
    /// Package this file belongs to
    pub package: Option<PackageRef>,
    pub path: PathBuf,
    pub properties: Properties,
    pub flags: FileFlags,
    /// Which provider this came from
    pub source: &'static str,
    /// Used to handle finding missing files when checking for unexpected files
    #[doc(hidden)]
    pub seen: AtomicBool,
    // Has one byte of padding left over for something else
}

impl Clone for FileEntry {
    fn clone(&self) -> Self {
        Self {
            package: self.package,
            path: self.path.clone(),
            properties: self.properties.clone(),
            flags: self.flags,
            source: self.source,
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
    /// Bitmask of flags for a file entry
    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    #[derive(serde::Serialize, serde::Deserialize)]
    pub struct FileFlags : u16 {
        /// This file is considered a configuration file by the package manager
        const CONFIG = 0b0000_0000_0000_0001;
        /// It is OK if this file is missing (currently only relevant for systemd-tmpfiles)
        const OK_IF_MISSING = 0b0000_0000_0000_0010;
    }
}

/// File properties from the package database(s)
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Properties {
    /// A regular file with just checksum info (as Debian gives us)
    RegularFileBasic(RegularFileBasic),
    /// A regular file with info that systemd-tmpfiles provides
    RegularFileSystemd(RegularFileSystemd),
    /// A regular file with all info (as Arch Linux has)
    RegularFile(RegularFile),
    Symlink(Symlink),
    Directory(Directory),
    Fifo(Fifo),
    DeviceNode(DeviceNode),
    /// This is some unknown thing that is not a file, symlink or directory
    /// (Currently generated in theory by Arch Linux backend, but no actual
    /// packages has this from what I can tell.)
    Special,
    /// An entry that shouldn't exist (being actively removed).
    /// (Currently only systemd-tmpfiles.)
    Removed,
    /// If the package management system doesn't give us enough info,
    /// all we know is that it should exist.
    Unknown,
    /// We don't know what it is, just what permissions it should have.
    /// (Currently only systemd-tmpfiles.)
    Permissions(Permissions),
}

impl Properties {
    #[must_use]
    pub fn is_regular_file(&self) -> Option<bool> {
        match self {
            Self::RegularFileBasic(_) => Some(true),
            Self::RegularFileSystemd(_) => Some(true),
            Self::RegularFile(_) => Some(true),
            Self::Symlink(_) => Some(false),
            Self::Directory(_) => Some(false),
            Self::Fifo(_) => Some(false),
            Self::DeviceNode(_) => Some(false),
            Self::Special => Some(false),
            Self::Removed => None,
            Self::Unknown => None,
            Self::Permissions(_) => None,
        }
    }

    #[must_use]
    pub fn is_dir(&self) -> Option<bool> {
        match self {
            Self::RegularFileBasic(_) => Some(false),
            Self::RegularFileSystemd(_) => Some(false),
            Self::RegularFile(_) => Some(false),
            Self::Symlink(_) => Some(false),
            Self::Directory(_) => Some(true),
            Self::Special => Some(false),
            Self::Fifo(_) => Some(false),
            Self::DeviceNode(_) => Some(false),
            Self::Removed => None,
            Self::Unknown => None,
            Self::Permissions(_) => None,
        }
    }

    /// Get mode (if available)
    #[must_use]
    pub fn mode(&self) -> Option<Mode> {
        match self {
            Self::RegularFileBasic(_) => None,
            Self::RegularFileSystemd(val) => Some(val.mode),
            Self::RegularFile(val) => Some(val.mode),
            Self::Symlink(_) => None,
            Self::Directory(val) => Some(val.mode),
            Self::Fifo(val) => Some(val.mode),
            Self::DeviceNode(val) => Some(val.mode),
            Self::Special => None,
            Self::Removed => None,
            Self::Unknown => None,
            Self::Permissions(val) => Some(val.mode),
        }
    }

    #[must_use]
    pub fn owner(&self) -> Option<Uid> {
        match self {
            Self::RegularFileBasic(_) => None,
            Self::RegularFileSystemd(val) => Some(val.owner),
            Self::RegularFile(val) => Some(val.owner),
            Self::Symlink(val) => Some(val.owner),
            Self::Directory(val) => Some(val.owner),
            Self::Fifo(val) => Some(val.owner),
            Self::DeviceNode(val) => Some(val.owner),
            Self::Special => None,
            Self::Removed => None,
            Self::Unknown => None,
            Self::Permissions(val) => Some(val.owner),
        }
    }

    #[must_use]
    pub fn group(&self) -> Option<Gid> {
        match self {
            Self::RegularFileBasic(_) => None,
            Self::RegularFileSystemd(val) => Some(val.group),
            Self::RegularFile(val) => Some(val.group),
            Self::Symlink(val) => Some(val.group),
            Self::Directory(val) => Some(val.group),
            Self::Fifo(val) => Some(val.group),
            Self::DeviceNode(val) => Some(val.group),
            Self::Special => None,
            Self::Removed => None,
            Self::Unknown => None,
            Self::Permissions(val) => Some(val.group),
        }
    }
}

/// A set of permissions
#[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub struct Permissions {
    pub mode: Mode,
    pub owner: Uid,
    pub group: Gid,
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_mode_parsing() {
        let mode = super::Mode::parse("u=rwx,g=rw,o=r").unwrap();
        assert_eq!(mode.as_raw(), 0o764);

        let mode = super::Mode::parse("u=,g=,o=").unwrap();
        assert_eq!(mode.as_raw(), 0);

        let mode = super::Mode::parse("u=rwx,g=,o=").unwrap();
        assert_eq!(mode.as_raw(), 0o700);

        let mode = super::Mode::parse("u=rwx,g=rwx,o=rwx").unwrap();
        assert_eq!(mode.as_raw(), 0o777);
    }
}
