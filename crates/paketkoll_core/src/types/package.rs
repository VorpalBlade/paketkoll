//! Types and traits for representing data about packages

use compact_str::CompactString;

/// Type of interner
pub type PackageInterner = lasso::ThreadedRodeo;

/// Newtype of interned string of package
///
/// This is used to cut down on allocations when tracking which package
/// each file belongs to.
///
/// Treat this as an opaque token
#[repr(transparent)]
#[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Clone, Copy)]
pub struct PackageRef(pub(crate) lasso::Spur);

impl PackageRef {
    /// Get a type suitable for use with the interner
    ///
    /// Specific type is not stable and public (i.e. what interner is used can change).
    pub fn as_interner_ref(&self) -> lasso::Spur {
        self.0
    }
}

/// The reason a package is installed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstallReason {
    Explicit,
    Dependency,
}

/// Describes a package as needed by paketkoll & related future tools
#[derive(Debug, PartialEq, Eq, Hash, Clone)]
#[non_exhaustive]
pub struct Package {
    /// Name of package
    pub name: PackageRef,
    /// Version of package
    pub version: CompactString,
    /// Single line description
    pub desc: CompactString,
    /// Dependencies (non-optional ones only)
    pub depends: Vec<PackageRef>,
    /// Names of provided/replaced packages
    pub provides: Vec<PackageRef>,
    /// Install reason
    pub reason: InstallReason,
}
