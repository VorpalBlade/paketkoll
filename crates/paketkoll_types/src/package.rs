//! Types and traits for representing data about packages

use compact_str::CompactString;
use smallvec::SmallVec;

use crate::intern::ArchitectureRef;
use crate::intern::Interner;
use crate::intern::PackageRef;

/// The reason a package is installed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum InstallReason {
    Explicit,
    Dependency,
}

/// The status of the installed package
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum PackageInstallStatus {
    /// Fully installed, as expected
    Installed,
    /// Some sort of partial install (not fully removed, error during install etc.)
    Partial,
}

/// Describes a package as needed by paketkoll & related future tools
///
/// This is generic over using interned types or direct strings. This is needed
/// to support generating JSON for serde, which can't deal with the interner.
#[derive(Debug, PartialEq, Eq, Clone, derive_builder::Builder)]
pub struct Package<PackageT, ArchitectureT>
where
    PackageT: std::fmt::Debug + PartialEq + Eq + Clone,
    ArchitectureT: std::fmt::Debug + PartialEq + Eq + Clone,
{
    /// Name of package
    pub name: PackageT,
    /// Architecture this package is for
    pub architecture: Option<ArchitectureT>,
    /// Version of package
    pub version: CompactString,
    /// Single line description
    #[builder(default = "None")]
    pub desc: Option<CompactString>,
    /// Dependencies (non-optional ones only)
    #[builder(default = "vec![]")]
    pub depends: Vec<Dependency<PackageT>>,
    /// Names of provided/replaced packages
    #[builder(default = "vec![]")]
    pub provides: Vec<PackageT>,
    /// Install reason
    #[builder(default = "None")]
    pub reason: Option<InstallReason>,
    /// Install status
    pub status: PackageInstallStatus,
    /// IDs for package (if not same as name).
    ///
    /// The first one should be the preferred or canonical one.
    #[builder(default = "smallvec::smallvec![]")]
    pub ids: SmallVec<[PackageT; 4]>,
}

/// Interned compact package
pub type PackageInterned = Package<PackageRef, ArchitectureRef>;
/// Package with strings in them, for serialisation purposes
pub type PackageDirect = Package<CompactString, CompactString>;

impl<PackageT, ArchitectureT> Package<PackageT, ArchitectureT>
where
    PackageT: std::fmt::Debug + PartialEq + Eq + Clone,
    ArchitectureT: std::fmt::Debug + PartialEq + Eq + Clone,
{
    pub fn canonical_id(&self) -> &PackageT {
        if self.ids.is_empty() {
            &self.name
        } else {
            &self.ids[0]
        }
    }
}

impl<PackageT, ArchitectureT> Package<PackageT, ArchitectureT>
where
    PackageT: std::fmt::Debug + PartialEq + Eq + Clone + Copy,
    ArchitectureT: std::fmt::Debug + PartialEq + Eq + Clone + Copy,
{
    pub fn builder() -> PackageBuilder<PackageRef, ArchitectureRef> {
        PackageBuilder::default()
    }
}

#[cfg(feature = "serde")]
impl PackageInterned {
    /// Convert to direct representation
    pub fn into_direct(self, interner: &Interner) -> PackageDirect {
        PackageDirect {
            name: self.name.to_str(interner).into(),
            architecture: self
                .architecture
                .and_then(|arch| arch.try_to_str(interner))
                .map(Into::into),
            version: self.version,
            desc: self.desc,
            depends: self
                .depends
                .into_iter()
                .map(|dep| dep.to_direct(interner))
                .collect(),
            provides: self
                .provides
                .into_iter()
                .flat_map(|pkg| pkg.try_to_str(interner).map(Into::into))
                .collect(),
            reason: self.reason,
            status: self.status,
            ids: self
                .ids
                .into_iter()
                .flat_map(|pkg| pkg.try_to_str(interner).map(Into::into))
                .collect(),
        }
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for PackageDirect {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct("Package", 8)?;
        state.serialize_field("name", &self.name)?;
        state.serialize_field("architecture", &self.architecture)?;
        state.serialize_field("version", &self.version)?;
        state.serialize_field("desc", &self.desc)?;
        state.serialize_field("depends", &self.depends)?;
        state.serialize_field("provides", &self.provides)?;
        state.serialize_field("reason", &self.reason)?;
        state.serialize_field("status", &self.status)?;
        state.serialize_field("id", &self.ids)?;
        state.end()
    }
}

/// Describe a dependency
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Dependency<PackageT>
where
    PackageT: std::fmt::Debug + PartialEq + Eq + Clone,
{
    /// A single dependency
    Single(PackageT),
    /// "Needs at least one of"
    Disjunction(Vec<PackageT>),
}

impl Dependency<PackageRef> {
    /// Format using string interner
    pub fn format(&self, interner: &Interner) -> String {
        match self {
            Dependency::Single(pkg) => pkg.to_str(interner).to_string(),
            Dependency::Disjunction(packages) => {
                let mut out = String::new();
                for (idx, pkg) in packages.iter().enumerate() {
                    if idx > 0 {
                        out.push_str(" | ");
                    }
                    out.push_str(pkg.to_str(interner));
                }
                out
            }
        }
    }

    #[cfg(feature = "serde")]
    fn to_direct(&self, interner: &Interner) -> Dependency<CompactString> {
        match self {
            Dependency::Single(pkg) => Dependency::Single(pkg.to_str(interner).into()),
            Dependency::Disjunction(packages) => Dependency::Disjunction(
                packages
                    .iter()
                    .map(|pkg| pkg.to_str(interner).into())
                    .collect(),
            ),
        }
    }
}

#[cfg(feature = "serde")]
impl serde::Serialize for Dependency<CompactString> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Dependency::Single(pkg) => serializer.serialize_str(pkg),
            Dependency::Disjunction(packages) => {
                serializer.serialize_newtype_variant("Dependency", 1, "or", &packages)
            }
        }
    }
}
