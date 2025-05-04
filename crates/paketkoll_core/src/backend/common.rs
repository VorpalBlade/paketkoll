use paketkoll_types::backend::Files;
use paketkoll_types::backend::Packages;
use paketkoll_types::intern::Interner;
use paketkoll_types::intern::PackageRef;
use std::fmt::Debug;

/// A backend that implements all operations
pub trait FullBackend: Files + Packages {}

/// Action to perform according to filter
#[derive(Debug)]
pub enum FilterAction {
    Exclude,
    Include,
}

/// A filter for which packages to load data for
pub enum PackageFilter {
    Everything,
    // Given a package name (without version), decide if we should process it
    FilterFunction(Box<dyn Fn(&str) -> FilterAction + Sync + Send>),
}

impl PackageFilter {
    /// Should we include this package?
    ///
    /// We do de-interning here, since the fast path is to just include
    /// everything.
    pub(crate) fn should_include_interned(&self, package: PackageRef, interner: &Interner) -> bool {
        match self {
            Self::Everything => true,
            Self::FilterFunction(f) => match f(package.as_str(interner)) {
                FilterAction::Include => true,
                FilterAction::Exclude => false,
            },
        }
    }
}

impl Debug for PackageFilter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Everything => write!(f, "Everything"),
            Self::FilterFunction(_) => write!(f, "FilterFunction(...)"),
        }
    }
}
