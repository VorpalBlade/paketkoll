use std::collections::BTreeMap;

use compact_str::CompactString;

use paketkoll_types::files::Mode;

use crate::FileContents;

/// An operation to be performed on a file system entry
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, strum::EnumDiscriminants)]
pub enum FsOp {
    /// Remove a file
    Remove,
    /// Restore a file to its original state
    Restore,

    // Creation
    /// Create a directory
    CreateDirectory,
    /// Create a regular file with the given contents
    CreateFile(FileContents),
    /// Create a symlink pointing to the given location
    CreateSymlink { target: camino::Utf8PathBuf },
    /// Create a FIFO
    CreateFifo,
    /// Create a block device
    CreateBlockDevice { major: u64, minor: u64 },
    /// Create a character device
    CreateCharDevice { major: u64, minor: u64 },

    // Metadata
    /// Set the mode of a file
    SetMode { mode: Mode },
    /// Set the owner of a file
    SetOwner { owner: CompactString },
    /// Set the group of a file
    SetGroup { group: CompactString },

    /// Special value for when we want to inform the user about extraneous entries in their config
    Comment,
}

impl std::fmt::Display for FsOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FsOp::Remove => write!(f, "remove"),
            FsOp::Restore => {
                write!(f, "restore (from package manager)")
            }
            FsOp::CreateDirectory => write!(f, "mkdir"),
            FsOp::CreateFile(contents) => write!(f, "create file (with {})", contents.checksum()),
            FsOp::CreateSymlink { target } => write!(f, "symlink to {target}"),
            FsOp::CreateFifo => write!(f, "mkfifo"),
            FsOp::CreateBlockDevice { .. } => write!(f, "mknod (block device)"),
            FsOp::CreateCharDevice { .. } => write!(f, "mknod (char device)"),
            FsOp::SetMode { mode } => write!(f, "chmod {mode}"),
            FsOp::SetOwner { owner } => write!(f, "chown {owner}"),
            FsOp::SetGroup { group } => write!(f, "chgrp {group}"),
            FsOp::Comment => write!(f, "COMMENT"),
        }
    }
}

/// An instruction for a file system change
#[derive(Debug, Clone)]
pub struct FsInstruction {
    /// Path to operate on
    pub path: camino::Utf8PathBuf,
    /// Operation to perform
    pub op: FsOp,
    /// Optional comment for saving purposes
    pub comment: Option<CompactString>,
}

impl PartialOrd for FsInstruction {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FsInstruction {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        match self.path.cmp(&other.path) {
            std::cmp::Ordering::Equal => {}
            ord => return ord,
        }
        self.op.cmp(&other.op)
    }
}

impl PartialEq for FsInstruction {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && self.op == other.op
    }
}

impl Eq for FsInstruction {}

/// Describes an operation to perform on a package
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum PkgOp {
    Uninstall,
    Install,
}

/// Identifying a package
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PkgIdent {
    /// Which package manager to use
    pub package_manager: paketkoll_types::backend::Backend,
    /// Specifier describing which package to install.
    /// Typically package name, but may be some other sort of identifier (e.g. for flatpak)
    pub identifier: CompactString,
}

/// An instruction for a package manager change
#[derive(Debug, Clone)]
pub struct PkgInstruction {
    /// Operation to perform on package
    pub op: PkgOp,
    /// Optional comment for saving purposes
    pub comment: Option<CompactString>,
}

impl PkgInstruction {
    // Toggle between install and uninstall
    pub fn inverted(&self) -> Self {
        Self {
            op: match self.op {
                PkgOp::Install => PkgOp::Uninstall,
                PkgOp::Uninstall => PkgOp::Install,
            },
            comment: self.comment.clone(),
        }
    }
}

/// Type of collection of package instructions
pub type PkgInstructions = BTreeMap<PkgIdent, PkgInstruction>;

impl PartialEq for PkgInstruction {
    fn eq(&self, other: &Self) -> bool {
        self.op == other.op
    }
}

impl Eq for PkgInstruction {}

impl PartialOrd for PkgInstruction {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for PkgInstruction {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.op.cmp(&other.op)
    }
}
