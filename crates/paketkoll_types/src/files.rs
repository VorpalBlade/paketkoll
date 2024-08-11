//! Types representing things about the file system

use std::fmt::Octal;
use std::path::Path;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::time::SystemTime;

use crate::intern::PackageRef;

/// Unix file mode (permissions)
#[derive(
    Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[repr(transparent)]
pub struct Mode(u32);

impl Mode {
    #[inline]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    #[inline]
    pub const fn as_raw(&self) -> u32 {
        self.0
    }

    /// Parse from u=rwx,g=rw,o=r format
    ///
    /// Since that is a diff
    pub fn parse(value: &str) -> anyhow::Result<Self> {
        let components = value.split(',');

        let mut mode: u32 = 0;
        for component in components {
            let component = component.as_bytes();
            let who = component
                .first()
                .ok_or_else(|| anyhow::anyhow!("Invalid mode: {value}"))?;
            if component[1] != b'=' {
                return Err(anyhow::anyhow!("Invalid mode: {value}"));
            }
            let perms = &component[2..];

            // Convert perms to bits
            let mut bits = 0;
            for perm in perms {
                match perm {
                    b'r' => bits |= 0b100,
                    b'w' => bits |= 0b010,
                    b'x' => bits |= 0b001,
                    _ => return Err(anyhow::anyhow!("Invalid mode: {value}")),
                }
            }

            // Shift the bits based on who
            match who {
                b'u' => mode |= bits << 6,
                b'g' => mode |= bits << 3,
                b'o' => mode |= bits,
                _ => return Err(anyhow::anyhow!("Invalid mode: {value}")),
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
        nix::sys::stat::Mode::from_bits_truncate(mode.0)
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
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    #[inline]
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
        nix::unistd::Uid::from_raw(uid.0)
    }
}

impl From<&Uid> for nix::unistd::Uid {
    fn from(uid: &Uid) -> Self {
        nix::unistd::Uid::from_raw(uid.0)
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
    pub fn new(id: u32) -> Self {
        Self(id)
    }

    #[inline]
    pub fn as_raw(&self) -> u32 {
        self.0
    }
}

impl From<Gid> for nix::unistd::Gid {
    fn from(gid: Gid) -> Self {
        nix::unistd::Gid::from_raw(gid.0)
    }
}

impl From<&Gid> for nix::unistd::Gid {
    fn from(gid: &Gid) -> Self {
        nix::unistd::Gid::from_raw(gid.0)
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
            DeviceType::Block => write!(f, "block"),
            DeviceType::Char => write!(f, "char"),
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
    pub fn is_regular_file(&self) -> Option<bool> {
        match self {
            Properties::RegularFileBasic(_) => Some(true),
            Properties::RegularFileSystemd(_) => Some(true),
            Properties::RegularFile(_) => Some(true),
            Properties::Symlink(_) => Some(false),
            Properties::Directory(_) => Some(false),
            Properties::Fifo(_) => Some(false),
            Properties::DeviceNode(_) => Some(false),
            Properties::Special => Some(false),
            Properties::Removed => None,
            Properties::Unknown => None,
            Properties::Permissions(_) => None,
        }
    }

    pub fn is_dir(&self) -> Option<bool> {
        match self {
            Properties::RegularFileBasic(_) => Some(false),
            Properties::RegularFileSystemd(_) => Some(false),
            Properties::RegularFile(_) => Some(false),
            Properties::Symlink(_) => Some(false),
            Properties::Directory(_) => Some(true),
            Properties::Special => Some(false),
            Properties::Fifo(_) => Some(false),
            Properties::DeviceNode(_) => Some(false),
            Properties::Removed => None,
            Properties::Unknown => None,
            Properties::Permissions(_) => None,
        }
    }

    /// Get mode (if available)
    pub fn mode(&self) -> Option<Mode> {
        match self {
            Properties::RegularFileBasic(_) => None,
            Properties::RegularFileSystemd(val) => Some(val.mode),
            Properties::RegularFile(val) => Some(val.mode),
            Properties::Symlink(_) => None,
            Properties::Directory(val) => Some(val.mode),
            Properties::Fifo(val) => Some(val.mode),
            Properties::DeviceNode(val) => Some(val.mode),
            Properties::Special => None,
            Properties::Removed => None,
            Properties::Unknown => None,
            Properties::Permissions(val) => Some(val.mode),
        }
    }

    pub fn owner(&self) -> Option<Uid> {
        match self {
            Properties::RegularFileBasic(_) => None,
            Properties::RegularFileSystemd(val) => Some(val.owner),
            Properties::RegularFile(val) => Some(val.owner),
            Properties::Symlink(val) => Some(val.owner),
            Properties::Directory(val) => Some(val.owner),
            Properties::Fifo(val) => Some(val.owner),
            Properties::DeviceNode(val) => Some(val.owner),
            Properties::Special => None,
            Properties::Removed => None,
            Properties::Unknown => None,
            Properties::Permissions(val) => Some(val.owner),
        }
    }

    pub fn group(&self) -> Option<Gid> {
        match self {
            Properties::RegularFileBasic(_) => None,
            Properties::RegularFileSystemd(val) => Some(val.group),
            Properties::RegularFile(val) => Some(val.group),
            Properties::Symlink(val) => Some(val.group),
            Properties::Directory(val) => Some(val.group),
            Properties::Fifo(val) => Some(val.group),
            Properties::DeviceNode(val) => Some(val.group),
            Properties::Special => None,
            Properties::Removed => None,
            Properties::Unknown => None,
            Properties::Permissions(val) => Some(val.group),
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
