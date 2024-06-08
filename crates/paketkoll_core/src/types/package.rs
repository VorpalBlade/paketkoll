//! Types and traits for representing data about packages

use compact_str::CompactString;

use super::FileEntry;

/// Type of interner
pub type Interner = lasso::ThreadedRodeo;

macro_rules! intern_newtype {
    ($name:ident) => {
        /// Newtype for interning
        ///
        /// Treat this as an opaque token
        #[repr(transparent)]
        #[derive(Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Clone, Copy)]
        pub struct $name(pub(crate) lasso::Spur);

        impl $name {
            /// Get a type suitable for use with the interner
            ///
            /// Specific type is not stable and public (i.e. what interner is used can change).
            pub fn as_interner_ref(&self) -> lasso::Spur {
                self.0
            }

            /// Convert to a string
            pub fn to_str<'interner>(&self, interner: &'interner Interner) -> &'interner str {
                interner.resolve(&self.as_interner_ref())
            }
        }
    };
}

intern_newtype!(PackageRef);
intern_newtype!(ArchitectureRef);

/// The reason a package is installed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InstallReason {
    Explicit,
    Dependency,
}

/// The status of the installed package
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PackageInstallStatus {
    /// Fully installed, as expected
    Installed,
    /// Some sort of partial install (not fully removed, error during install etc)
    Partial,
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) struct BackendData {
    pub(crate) files: Vec<FileEntry>,
    pub(crate) packages: Vec<Package>,
}

/// Describes a package as needed by paketkoll & related future tools
#[derive(Debug, PartialEq, Eq, Hash, Clone, derive_builder::Builder)]
#[non_exhaustive]
pub struct Package {
    /// Name of package
    pub name: PackageRef,
    /// Architecture this package is for
    pub architecture: Option<ArchitectureRef>,
    /// Version of package
    pub version: CompactString,
    /// Single line description
    pub desc: CompactString,
    /// Dependencies (non-optional ones only)
    #[builder(default = "vec![]")]
    pub depends: Vec<Dependency>,
    /// Names of provided/replaced packages
    #[builder(default = "vec![]")]
    pub provides: Vec<PackageRef>,
    /// Install reason
    #[builder(default = "None")]
    pub reason: Option<InstallReason>,
    /// Install status
    pub status: PackageInstallStatus,
}

impl Package {
    pub(crate) fn builder() -> PackageBuilder {
        PackageBuilder::default()
    }
}

/// Describe a dependency
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Dependency {
    /// A single dependency
    Single(PackageRef),
    /// "Needs at least one of"
    Disjunction(Vec<PackageRef>),
}

impl Dependency {
    /// Format using string interner
    pub fn format(&self, interner: &Interner) -> String {
        match self {
            Dependency::Single(pkg) => interner.resolve(&pkg.as_interner_ref()).to_string(),
            Dependency::Disjunction(packages) => {
                let mut out = String::new();
                for (idx, pkg) in packages.iter().enumerate() {
                    if idx > 0 {
                        out.push_str(" | ");
                    }
                    out.push_str(interner.resolve(&pkg.as_interner_ref()));
                }
                out
            }
        }
    }
}
