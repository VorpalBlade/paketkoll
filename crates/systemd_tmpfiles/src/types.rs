use compact_str::CompactString;

/// All the operations systemd-tmpfiles supports (as of systemd 256)
///
/// The type specifiers are not mapped 1:1 but as it makes sense (as several
/// are very similar). This means that f and f+ for example are both mapped to
/// [`Directive::CreateFile`] with fields to differentiate between the two.
/// Similarly, all of v, q and Q are mapped to [`Directive::CreateSubvolume`],
/// and so on.
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
        /// `None` corresponds to v, and `Some(_)` corresponds to the two
        /// variants of q/Q.
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
    #[must_use]
    pub fn mode(&self) -> Option<&Mode> {
        match self {
            Self::CreateFile { mode, .. } => mode.as_ref(),
            Self::WriteToFile { .. } => None,
            Self::CreateDirectory { mode, .. } => mode.as_ref(),
            Self::AdjustPermissionsAndTmpFiles { mode, .. } => mode.as_ref(),
            Self::CreateSubvolume { mode, .. } => mode.as_ref(),
            Self::CreateFifo { mode, .. } => mode.as_ref(),
            Self::CreateSymlink { .. } => None,
            Self::CreateCharDeviceNode { mode, .. } => mode.as_ref(),
            Self::CreateBlockDeviceNode { mode, .. } => mode.as_ref(),
            Self::RecursiveCopy { .. } => None,
            Self::IgnorePathDuringCleaning { .. } => None,
            Self::IgnoreDirectoryDuringCleaning { .. } => None,
            Self::RemoveFile { .. } => None,
            Self::AdjustAccess { mode, .. } => mode.as_ref(),
            Self::SetExtendedAttributes { .. } => None,
            Self::SetAttributes { .. } => None,
            Self::SetAcl { .. } => None,
        }
    }

    #[must_use]
    pub fn user(&self) -> Option<&Id> {
        match self {
            Self::CreateFile { user, .. } => Some(user),
            Self::WriteToFile { .. } => None,
            Self::CreateDirectory { user, .. } => Some(user),
            Self::AdjustPermissionsAndTmpFiles { user, .. } => Some(user),
            Self::CreateSubvolume { user, .. } => Some(user),
            Self::CreateFifo { user, .. } => Some(user),
            Self::CreateSymlink { .. } => None,
            Self::CreateCharDeviceNode { user, .. } => Some(user),
            Self::CreateBlockDeviceNode { user, .. } => Some(user),
            Self::RecursiveCopy { .. } => None,
            Self::IgnorePathDuringCleaning { .. } => None,
            Self::IgnoreDirectoryDuringCleaning { .. } => None,
            Self::RemoveFile { .. } => None,
            Self::AdjustAccess { user, .. } => Some(user),
            Self::SetExtendedAttributes { .. } => None,
            Self::SetAttributes { .. } => None,
            Self::SetAcl { .. } => None,
        }
    }

    #[must_use]
    pub fn group(&self) -> Option<&Id> {
        match self {
            Self::CreateFile { group, .. } => Some(group),
            Self::WriteToFile { .. } => None,
            Self::CreateDirectory { group, .. } => Some(group),
            Self::AdjustPermissionsAndTmpFiles { group, .. } => Some(group),
            Self::CreateSubvolume { group, .. } => Some(group),
            Self::CreateFifo { group, .. } => Some(group),
            Self::CreateSymlink { .. } => None,
            Self::CreateCharDeviceNode { group, .. } => Some(group),
            Self::CreateBlockDeviceNode { group, .. } => Some(group),
            Self::RecursiveCopy { .. } => None,
            Self::IgnorePathDuringCleaning { .. } => None,
            Self::IgnoreDirectoryDuringCleaning { .. } => None,
            Self::RemoveFile { .. } => None,
            Self::AdjustAccess { group, .. } => Some(group),
            Self::SetExtendedAttributes { .. } => None,
            Self::SetAttributes { .. } => None,
            Self::SetAcl { .. } => None,
        }
    }

    /// True if the path could be a glob for this type of directive
    fn can_be_glob(&self) -> bool {
        match self {
            Self::CreateFile { .. } => false,
            Self::WriteToFile { .. } => true,
            Self::CreateDirectory { .. } => false,
            Self::AdjustPermissionsAndTmpFiles { .. } => true,
            Self::CreateSubvolume { .. } => false,
            Self::CreateFifo { .. } => false,
            Self::CreateSymlink { .. } => false,
            Self::CreateCharDeviceNode { .. } => false,
            Self::CreateBlockDeviceNode { .. } => false,
            Self::RecursiveCopy { .. } => false,
            Self::IgnorePathDuringCleaning { .. } => true,
            Self::IgnoreDirectoryDuringCleaning { .. } => true,
            Self::RemoveFile { .. } => true,
            Self::AdjustAccess { .. } => true,
            Self::SetExtendedAttributes { .. } => true,
            Self::SetAttributes { .. } => true,
            Self::SetAcl { .. } => true,
        }
    }
}

/// A single entry (line) from systemd-tmpfiles.d
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Entry {
    pub(crate) path: CompactString,
    pub(crate) directive: Directive,
    pub(crate) flags: EntryFlags,
}

impl Entry {
    /// Get the path of the entry
    #[must_use]
    pub fn path(&self) -> &str {
        self.path.as_str()
    }

    /// True if the path appears to be a glob
    #[must_use]
    pub fn path_is_glob(&self) -> bool {
        self.directive.can_be_glob() && self.path().contains(['*', '?', '['])
    }

    /// Get the directive of the entry
    #[must_use]
    pub fn directive(&self) -> &Directive {
        &self.directive
    }

    /// Get the flags of the entry
    #[must_use]
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
    #[must_use]
    pub fn new_only(&self) -> bool {
        match self {
            Self::Set { new_only, .. } => *new_only,
        }
    }

    #[must_use]
    pub fn mode(&self) -> libc::mode_t {
        match self {
            Self::Set { mode, .. } => *mode,
        }
    }
}

/// UID or GID
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
#[allow(variant_size_differences)]
pub enum Id {
    /// User or group will be set to caller of systemd-tmpfiles
    Caller { new_only: bool },
    /// UID or GID will be set
    Numeric { id: libc::uid_t, new_only: bool },
    /// User or group name will be set
    Name { name: CompactString, new_only: bool },
}

impl Default for Id {
    fn default() -> Self {
        const {
            assert!(size_of::<libc::uid_t>() == size_of::<libc::gid_t>());
        };
        Self::Caller { new_only: false }
    }
}

impl Id {
    #[must_use]
    pub fn new_only(&self) -> bool {
        match self {
            Self::Caller { new_only } => *new_only,
            Self::Numeric { new_only, .. } => *new_only,
            Self::Name { new_only, .. } => *new_only,
        }
    }
}

/// Age specifier for cleanup
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub struct Age {
    /// This field is complicated, and we don't need to parse it for our use
    /// case.
    pub(crate) specifier: CompactString,
}

impl Age {
    /// Get the raw specifier string
    #[must_use]
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
    pub(crate) fn try_from_bytes(value: &[u8]) -> Option<Self> {
        let value = std::str::from_utf8(value).ok()?;
        let parts = value.split_once(':')?;
        let major = parts.0.parse().ok()?;
        let minor = parts.1.parse().ok()?;
        Some(Self { major, minor })
    }
}
