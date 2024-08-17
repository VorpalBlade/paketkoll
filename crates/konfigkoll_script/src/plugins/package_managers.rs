//! Access to system package manager

use super::error::KResult;
use eyre::WrapErr;
use paketkoll_types::backend::Backend;
use paketkoll_types::backend::Files;
use paketkoll_types::backend::OriginalFileQuery;
use paketkoll_types::backend::PackageBackendMap;
use paketkoll_types::backend::PackageMap;
use paketkoll_types::backend::PackageMapMap;
use paketkoll_types::backend::Packages;
use paketkoll_types::intern::Interner;
use rune::alloc::fmt::TryWrite;
use rune::runtime::Bytes;
use rune::runtime::Shared;
use rune::vm_write;
use rune::Any;
use rune::ContextError;
use rune::Module;
use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;

/// Type of map for package managers
pub type PackageManagerMap = BTreeMap<Backend, PackageManager>;

#[derive(Debug, Any)]
#[rune(item = ::package_managers)]
/// The collection of enabled package managers
pub struct PackageManagers {
    package_managers: PackageManagerMap,
    backend_with_files: Backend,
}

impl PackageManagers {
    /// Create a new package managers
    pub fn create_from(
        package_backends: &PackageBackendMap,
        file_backend_id: Backend,
        files_backend: &Arc<dyn Files>,
        package_maps: &PackageMapMap,
        interner: &Arc<Interner>,
    ) -> Self {
        let files_backends = [(file_backend_id, files_backend)];
        // Join all three maps on key. This is equivalent to a SQL outer join.
        // Use itertools::merge_join_by for this.
        let merged =
            itertools::merge_join_by(package_backends, files_backends, |l, r| l.0.cmp(&r.0));
        // We now know that all keys are present (everything is a package, file or both
        // backend)
        let mut package_managers = PackageManagerMap::new();
        for entry in merged {
            let (backend, packages, files) = match entry {
                itertools::EitherOrBoth::Both(a, b) => (*a.0, Some(a.1), Some(b.1)),
                itertools::EitherOrBoth::Left(a) => (*a.0, Some(a.1), None),
                itertools::EitherOrBoth::Right(b) => (b.0, None, Some(b.1)),
            };

            let package_map = package_maps.get(&backend).cloned();
            let pkg_mgr = PackageManager::new(
                backend,
                files.cloned(),
                packages.cloned(),
                package_map,
                interner.clone(),
            );
            package_managers.insert(backend, pkg_mgr);
        }
        Self {
            package_managers,
            backend_with_files: file_backend_id,
        }
    }
}

impl PackageManagers {
    /// Get an instance of a [`PackageManager`] by backend name
    #[rune::function]
    fn get(&self, name: &str) -> Option<PackageManager> {
        let backend = Backend::from_str(name).ok()?;
        self.package_managers.get(&backend).cloned()
    }

    /// Get the package manager that handles files
    #[rune::function]
    fn files(&self) -> PackageManager {
        self.package_managers
            .get(&self.backend_with_files)
            .expect("There should always be a files backend")
            .clone()
    }
}

/// Inner struct because rune function attributes don't want to play along.
#[derive(Debug, Clone)]
struct PackageManagerInner {
    backend: Backend,
    files: Option<Arc<dyn Files>>,
    packages: Option<Arc<dyn Packages>>,
    package_map: Option<Arc<PackageMap>>,
    interner: Arc<Interner>,
}

#[derive(Debug, Clone, Any)]
#[rune(item = ::package_managers)]
#[repr(transparent)]
/// A package manager
pub struct PackageManager {
    inner: Shared<PackageManagerInner>,
}

// Rust API
impl PackageManager {
    /// Create a new package manager
    pub fn new(
        backend: Backend,
        files: Option<Arc<dyn Files>>,
        packages: Option<Arc<dyn Packages>>,
        package_map: Option<Arc<PackageMap>>,
        interner: Arc<Interner>,
    ) -> Self {
        Self {
            inner: Shared::new(PackageManagerInner {
                backend,
                files,
                packages,
                package_map,
                interner,
            })
            .expect("Failed to create shared package manager"),
        }
    }

    pub fn files(&self) -> Option<Arc<dyn Files>> {
        self.inner.borrow_ref().ok()?.files.clone()
    }

    pub fn packages(&self) -> Option<Arc<dyn Packages>> {
        self.inner.borrow_ref().ok()?.packages.clone()
    }

    /// Get the original file contents of a package from Rust code
    pub fn file_contents(&self, package: &str, path: &str) -> Result<Vec<u8>, OriginalFilesError> {
        let queries: [_; 1] = [OriginalFileQuery {
            package: package.into(),
            path: path.into(),
        }];
        let guard = self
            .inner
            .borrow_ref()
            .wrap_err("Failed to get inner object")?;
        let files = guard
            .files
            .as_ref()
            .ok_or_else(|| eyre::eyre!("No files backend for {}", guard.backend))?;
        let package_map = guard
            .package_map
            .as_ref()
            .ok_or_else(|| eyre::eyre!("No package map for {}", guard.backend))?;
        let results = files
            .original_files(&queries, package_map, &guard.interner)
            .map_err(|e| OriginalFilesError::from_inner(e, path))?;
        if results.len() != 1 {
            Err(eyre::eyre!(
                "Failed original_file_contents({package}, {path}): Got wrong number of results: {}",
                results.len()
            ))?;
        }
        let result = results
            .into_iter()
            .next()
            .ok_or_else(|| {
                eyre::eyre!(
                    "Failed original_file_contents({package}, {path}): Failed to extract result"
                )
            })?
            .1;
        Ok(result)
    }
}

// Rune API
impl PackageManager {
    /// Get the original file contents of a package as a `Result<Bytes>`
    #[rune::function(keep)]
    fn original_file_contents(&self, package: &str, path: &str) -> KResult<Bytes> {
        let result = self
            .file_contents(package, path)
            .wrap_err("Failed to get original file contents")?;
        Ok(Bytes::from_vec(
            result
                .try_into()
                .wrap_err("Failed to get original file contents")?,
        ))
    }
}

#[derive(Debug, Any, thiserror::Error)]
#[rune(item = ::package_managers)]
pub enum OriginalFilesError {
    #[error("Package not found: {0}")]
    PackageNotFound(#[rune(get)] String),
    #[error("File {1} not found in package: {0}")]
    FileNotFound(#[rune(get)] String, #[rune(get)] String),
    #[error("Failed to get original file: {0}")]
    Other(#[from] eyre::Error),
}

impl OriginalFilesError {
    #[rune::function(vm_result, protocol = STRING_DEBUG)]
    fn string_debug(&self, f: &mut rune::runtime::Formatter) {
        vm_write!(f, "{:?}", self);
    }

    fn from_inner(value: paketkoll_types::backend::OriginalFileError, path: &str) -> Self {
        match value {
            paketkoll_types::backend::OriginalFileError::PackageNotFound(v) => {
                Self::PackageNotFound(v.into())
            }
            paketkoll_types::backend::OriginalFileError::FileNotFound(v) => {
                Self::FileNotFound(v.into(), path.into())
            }
            paketkoll_types::backend::OriginalFileError::Other(v) => Self::Other(v),
        }
    }
}

#[rune::module(::package_managers)]
/// Interface to the package manager(s) in the system
pub(crate) fn module() -> Result<Module, ContextError> {
    let mut m = Module::from_meta(self::module_meta)?;
    m.ty::<PackageManager>()?;
    m.function_meta(PackageManager::original_file_contents__meta)?;
    m.ty::<PackageManagers>()?;
    m.function_meta(PackageManagers::get)?;
    m.function_meta(PackageManagers::files)?;
    m.ty::<OriginalFilesError>()?;
    Ok(m)
}
