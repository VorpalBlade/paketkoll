//! Types representing things about the file system

/// Unix file mode (permissions)
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(transparent)]
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
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(transparent)]
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
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[repr(transparent)]
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

/// Represents a checksum of a file
///
/// Which checksum types are used depend on the feature flags.
/// For example currently: Arch uses SHA256, and Debian uses MD5.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
#[non_exhaustive]
pub enum Checksum {
    #[cfg_attr(
        feature = "serde",
        serde(serialize_with = "crate::utils::buffer_to_hex")
    )]
    Md5([u8; 16]),
    #[cfg_attr(
        feature = "serde",
        serde(serialize_with = "crate::utils::buffer_to_hex")
    )]
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
