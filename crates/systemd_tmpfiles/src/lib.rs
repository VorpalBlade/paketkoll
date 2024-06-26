//! A parser for systemd-tmpfiles configuration files

use compact_str::CompactString;

mod architecture;
pub mod parser;
pub mod specifier;

/// All the operations systemd-tmpfiles supports (as of systemd 256)
///
/// The type specifiers are not mapped 1:1 but as it makes sense (as several
/// are very similar). This means that f and f+ for example are both mapped to
/// [`Directive::CreateFile`] with fields to differentiate between the two.
/// Similarly all of v, q and Q are mapped to [`Directive::CreateSubvolume`],
/// and so on.
///
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Directive {
    /// f(+)
    CreateFile {
        truncate_if_exists: bool,
        mode: Option<Mode>,
        user: Id,
        group: Id,
        /// This is the argument field (last field)
        contents: Option<Box<[u8]>>,
    },
    /// w(+)
    WriteToFile {
        append: bool,
        /// This is the argument fields (last field)
        contents: Box<[u8]>,
    },
    /// d/D
    CreateDirectory {
        remove_if_exists: bool,
        mode: Option<Mode>,
        user: Id,
        group: Id,
        cleanup_age: Option<Age>,
    },
    /// e (systemd 230+)
    AdjustPermissionsAndTmpFiles {
        mode: Option<Mode>,
        user: Id,
        group: Id,
        cleanup_age: Option<Age>,
    },
    /// v (systemd 219+) / q/Q (systemd 228+)
    CreateSubvolume {
        /// `None` corresponds to v, and `Some(_)` corresponds to the two variants of q/Q.
        quota: Option<SubvolumeQuota>,
        mode: Option<Mode>,
        user: Id,
        group: Id,
        cleanup_age: Option<Age>,
    },
    /// p(+)
    CreateFifo {
        replace_if_exists: bool,
        mode: Option<Mode>,
        user: Id,
        group: Id,
    },
    /// L(+)
    CreateSymlink {
        replace_if_exists: bool,
        /// This is the argument fields (last field)
        target: Option<Box<[u8]>>,
    },
    /// c(+)
    CreateCharDeviceNode {
        replace_if_exists: bool,
        mode: Option<Mode>,
        user: Id,
        group: Id,
        /// This is the argument fields (last field)
        device_specifier: DeviceNode,
    },
    /// b(+)
    CreateBlockDeviceNode {
        replace_if_exists: bool,
        mode: Option<Mode>,
        user: Id,
        group: Id,
        /// This is the argument fields (last field)
        device_specifier: DeviceNode,
    },
    /// C(+)
    RecursiveCopy {
        recursive_if_exists: bool,
        cleanup_age: Option<Age>,
        /// This is the argument fields (last field)
        source: Option<CompactString>,
    },
    /// x
    IgnorePathDuringCleaning { cleanup_age: Option<Age> },
    /// X (systemd 198+)
    IgnoreDirectoryDuringCleaning { cleanup_age: Option<Age> },
    /// r/R
    RemoveFile { recursive: bool },
    /// z/Z
    AdjustAccess {
        recursive: bool,
        mode: Option<Mode>,
        user: Id,
        group: Id,
    },
    /// t/T (systemd 218+)
    SetExtendedAttributes {
        recursive: bool,
        /// This is the argument fields (last field)
        attributes: Box<[u8]>,
    },
    /// h/H
    SetAttributes {
        recursive: bool,
        /// This is the argument fields (last field)
        attributes: Box<[u8]>,
    },
    /// a/A(+)
    SetAcl {
        recursive: bool,
        append: bool,
        /// This is the argument fields (last field)
        acls: Box<[u8]>,
    },
}

impl Directive {
    pub fn mode(&self) -> Option<&Mode> {
        match self {
            Directive::CreateFile { mode, .. } => mode.as_ref(),
            Directive::WriteToFile { .. } => None,
            Directive::CreateDirectory { mode, .. } => mode.as_ref(),
            Directive::AdjustPermissionsAndTmpFiles { mode, .. } => mode.as_ref(),
            Directive::CreateSubvolume { mode, .. } => mode.as_ref(),
            Directive::CreateFifo { mode, .. } => mode.as_ref(),
            Directive::CreateSymlink { .. } => None,
            Directive::CreateCharDeviceNode { mode, .. } => mode.as_ref(),
            Directive::CreateBlockDeviceNode { mode, .. } => mode.as_ref(),
            Directive::RecursiveCopy { .. } => None,
            Directive::IgnorePathDuringCleaning { .. } => None,
            Directive::IgnoreDirectoryDuringCleaning { .. } => None,
            Directive::RemoveFile { .. } => None,
            Directive::AdjustAccess { mode, .. } => mode.as_ref(),
            Directive::SetExtendedAttributes { .. } => None,
            Directive::SetAttributes { .. } => None,
            Directive::SetAcl { .. } => None,
        }
    }

    pub fn user(&self) -> Option<&Id> {
        match self {
            Directive::CreateFile { user, .. } => Some(user),
            Directive::WriteToFile { .. } => None,
            Directive::CreateDirectory { user, .. } => Some(user),
            Directive::AdjustPermissionsAndTmpFiles { user, .. } => Some(user),
            Directive::CreateSubvolume { user, .. } => Some(user),
            Directive::CreateFifo { user, .. } => Some(user),
            Directive::CreateSymlink { .. } => None,
            Directive::CreateCharDeviceNode { user, .. } => Some(user),
            Directive::CreateBlockDeviceNode { user, .. } => Some(user),
            Directive::RecursiveCopy { .. } => None,
            Directive::IgnorePathDuringCleaning { .. } => None,
            Directive::IgnoreDirectoryDuringCleaning { .. } => None,
            Directive::RemoveFile { .. } => None,
            Directive::AdjustAccess { user, .. } => Some(user),
            Directive::SetExtendedAttributes { .. } => None,
            Directive::SetAttributes { .. } => None,
            Directive::SetAcl { .. } => None,
        }
    }

    pub fn group(&self) -> Option<&Id> {
        match self {
            Directive::CreateFile { group, .. } => Some(group),
            Directive::WriteToFile { .. } => None,
            Directive::CreateDirectory { group, .. } => Some(group),
            Directive::AdjustPermissionsAndTmpFiles { group, .. } => Some(group),
            Directive::CreateSubvolume { group, .. } => Some(group),
            Directive::CreateFifo { group, .. } => Some(group),
            Directive::CreateSymlink { .. } => None,
            Directive::CreateCharDeviceNode { group, .. } => Some(group),
            Directive::CreateBlockDeviceNode { group, .. } => Some(group),
            Directive::RecursiveCopy { .. } => None,
            Directive::IgnorePathDuringCleaning { .. } => None,
            Directive::IgnoreDirectoryDuringCleaning { .. } => None,
            Directive::RemoveFile { .. } => None,
            Directive::AdjustAccess { group, .. } => Some(group),
            Directive::SetExtendedAttributes { .. } => None,
            Directive::SetAttributes { .. } => None,
            Directive::SetAcl { .. } => None,
        }
    }

    /// True if the path could be a glob for this type of directive
    fn can_be_glob(&self) -> bool {
        match self {
            Directive::CreateFile { .. } => false,
            Directive::WriteToFile { .. } => true,
            Directive::CreateDirectory { .. } => false,
            Directive::AdjustPermissionsAndTmpFiles { .. } => true,
            Directive::CreateSubvolume { .. } => false,
            Directive::CreateFifo { .. } => false,
            Directive::CreateSymlink { .. } => false,
            Directive::CreateCharDeviceNode { .. } => false,
            Directive::CreateBlockDeviceNode { .. } => false,
            Directive::RecursiveCopy { .. } => false,
            Directive::IgnorePathDuringCleaning { .. } => true,
            Directive::IgnoreDirectoryDuringCleaning { .. } => true,
            Directive::RemoveFile { .. } => true,
            Directive::AdjustAccess { .. } => true,
            Directive::SetExtendedAttributes { .. } => true,
            Directive::SetAttributes { .. } => true,
            Directive::SetAcl { .. } => true,
        }
    }
}

/// A single entry (line) from systemd-tmpfiles.d
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Entry {
    path: CompactString,
    directive: Directive,
    flags: EntryFlags,
}

impl Entry {
    /// Get the path of the entry
    pub fn path(&self) -> &str {
        self.path.as_str()
    }

    /// True if the path appears to be a glob
    pub fn path_is_glob(&self) -> bool {
        self.directive.can_be_glob() && self.path().contains(|c| c == '*' || c == '?' || c == '[')
    }

    /// Get the directive of the entry
    pub fn directive(&self) -> &Directive {
        &self.directive
    }

    /// Get the flags of the entry
    pub fn flags(&self) -> EntryFlags {
        self.flags
    }
}

bitflags::bitflags! {
    /// Various flags for an entry
    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    #[non_exhaustive]
    pub struct EntryFlags: u8 {
        /// (!) Only apply entry at boot
        const BOOT_ONLY = 0b01;
        /// (-) Not an error if it didn't exist
        const ERRORS_OK_ON_CREATE = 0b10;
        /// (=) Remove non-matching entries
        const REMOVE_NONMATCHING = 0b100;
        /// (~) Column 6 is encoded as base64
        const ARG_BASE64 = 0b1000;
        /// (^) Column 6 is a credential
        const ARG_CREDENTIAL = 0b10000;
    }
}

// Notes:
// * Modes also have modifiers
// * User/group have a format
// * Age has a format
// * Path/argument can use specifiers, e.g. %a

/// Unix mode
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Mode {
    /// Mode will be overwritten
    Set {
        /// Mode to set
        mode: libc::mode_t,
        /// Only apply on new inodes
        new_only: bool,
        /// Mask mode based on existing bits
        masked: bool,
    },
}

impl Mode {
    pub fn new_only(&self) -> bool {
        match self {
            Mode::Set { new_only, .. } => *new_only,
        }
    }
    pub fn mode(&self) -> libc::mode_t {
        match self {
            Mode::Set { mode, .. } => *mode,
        }
    }
}

/// UID or GID
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum Id {
    /// User or group will be set to caller of systemd-tmpfiles
    Caller { new_only: bool },
    /// UID or GID will be set
    Id { id: libc::uid_t, new_only: bool },
    /// User or group name will be set
    Name { name: CompactString, new_only: bool },
}

impl Default for Id {
    fn default() -> Self {
        const {
            assert!(std::mem::size_of::<libc::uid_t>() == std::mem::size_of::<libc::gid_t>());
        };
        Self::Caller { new_only: false }
    }
}

impl Id {
    pub fn new_only(&self) -> bool {
        match self {
            Id::Caller { new_only } => *new_only,
            Id::Id { new_only, .. } => *new_only,
            Id::Name { new_only, .. } => *new_only,
        }
    }
}

/// Age specifier for cleanup
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Age {
    /// This field is complicated, and we don't need to parse it for our use case.
    specifier: CompactString,
}

impl Age {
    /// Get the raw specifier string
    pub fn raw(&self) -> &str {
        self.specifier.as_str()
    }
}

/// Describes quota handling for subvolumes in v, q & Q directives
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum SubvolumeQuota {
    Inherit,
    New,
}

/// Device node specifier
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeviceNode {
    /// Major number
    pub major: libc::dev_t,
    /// Minor number
    pub minor: libc::dev_t,
}

impl DeviceNode {
    fn try_from_bytes(value: &[u8]) -> Option<Self> {
        let value = std::str::from_utf8(value).ok()?;
        let parts = value.split_once(':')?;
        let major = parts.0.parse().ok()?;
        let minor = parts.1.parse().ok()?;
        Some(Self { major, minor })
    }
}
