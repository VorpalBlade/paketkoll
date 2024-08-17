//! Wrapping backend that performs disk cache of original files queries

use std::fmt::Debug;
use std::fmt::Display;
use std::path::Path;

use ahash::AHashMap;
use cached::stores::DiskCacheBuilder;
use cached::DiskCache;
use cached::IOCached;
use compact_str::CompactString;
use eyre::Context;
use paketkoll_types::backend::ArchiveResult;
use paketkoll_types::backend::Backend;
use paketkoll_types::backend::Files;
use paketkoll_types::backend::Name;
use paketkoll_types::backend::OriginalFileError;
use paketkoll_types::backend::OriginalFileQuery;
use paketkoll_types::backend::OriginalFilesResult;
use paketkoll_types::backend::PackageManagerError;
use paketkoll_types::backend::PackageMap;
use paketkoll_types::files::FileEntry;
use paketkoll_types::intern::Interner;
use paketkoll_types::intern::PackageRef;

use crate::utils::format_package;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    backend: &'static str,
    cache_version: u16,
    package: CompactString,
    path: CompactString,
}

impl CacheKey {
    pub fn new(
        backend: &'static str,
        cache_version: u16,
        package: CompactString,
        path: CompactString,
    ) -> Self {
        Self {
            backend,
            cache_version,
            package,
            path,
        }
    }
}

impl Display for CacheKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}:{}",
            self.backend, self.cache_version, self.package, self.path
        )
    }
}

pub struct OriginalFilesCache {
    inner: Box<dyn Files>,
    cache: DiskCache<CacheKey, Vec<u8>>,
}

impl OriginalFilesCache {
    pub fn from_path(inner: Box<dyn Files>, path: &Path) -> eyre::Result<Self> {
        let cache = DiskCacheBuilder::new("original_files")
            .set_refresh(true)
            .set_lifespan(60 * 60 * 24 * 30) // A month
            .set_disk_directory(path)
            .build()?;
        Ok(Self { inner, cache })
    }
}

impl Debug for OriginalFilesCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OriginalFilesCache")
            .field("inner", &self.inner)
            .field("cache", &"DiskCache<OriginalFileQuery, Vec<u8>>")
            .finish()
    }
}

impl Name for OriginalFilesCache {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn as_backend_enum(&self) -> Backend {
        self.inner.as_backend_enum()
    }
}

impl Files for OriginalFilesCache {
    fn files(&self, interner: &Interner) -> eyre::Result<Vec<FileEntry>> {
        self.inner.files(interner)
    }

    fn may_need_canonicalization(&self) -> bool {
        self.inner.may_need_canonicalization()
    }

    fn owning_packages(
        &self,
        paths: &ahash::AHashSet<&Path>,
        interner: &Interner,
    ) -> eyre::Result<dashmap::DashMap<std::path::PathBuf, Option<PackageRef>, ahash::RandomState>>
    {
        self.inner.owning_packages(paths, interner)
    }

    fn original_files(
        &self,
        queries: &[OriginalFileQuery],
        packages: &PackageMap,
        interner: &Interner,
    ) -> Result<OriginalFilesResult, OriginalFileError> {
        // Build up lists of cached and uncached queries
        let mut results = OriginalFilesResult::new();
        let mut uncached_queries = Vec::new();
        let mut cache_keys = AHashMap::new();
        let inner_name = self.name();
        let cache_version = self.cache_version();
        for query in queries.iter() {
            // Resolve exact version and ID of packages from the package map
            let cache_key = match packages.get(&PackageRef::get_or_intern(interner, &query.package))
            {
                Some(p) => format_package(p, interner),
                None => {
                    tracing::warn!(
                        "Package not found (likely not installed): {}",
                        query.package
                    );
                    uncached_queries.push(query.clone());
                    continue;
                }
            };
            let cache_key = CacheKey::new(inner_name, cache_version, cache_key, query.path.clone());
            match self
                .cache
                .cache_get(&cache_key)
                .context("Failed cache query")?
            {
                Some(v) => {
                    tracing::trace!("Cache hit: {}", cache_key);
                    results.insert(query.clone(), v);
                }
                None => {
                    tracing::trace!("Cache miss: {}", cache_key);
                    uncached_queries.push(query.clone());
                    cache_keys.insert(query.clone(), cache_key);
                }
            }
        }
        // Fetch uncached queries
        let uncached_results = self
            .inner
            .original_files(&uncached_queries, packages, interner)
            .with_context(|| format!("Inner query of {uncached_queries:?} failed"))?;

        // Insert the uncached results into the cache and update the results
        for (query, result) in uncached_results.into_iter() {
            match cache_keys.remove(&query) {
                Some(cache_key) => {
                    self.cache
                        .cache_set(cache_key, result.clone())
                        .context("Failed cache insertion")?;
                }
                None => {
                    tracing::warn!(
                        "Could not find cache key for query \"{query:?}\", will not be able to \
                         cache it (likely cause: providing package is not (yet) installed)",
                    );
                }
            };
            results.insert(query, result);
        }

        Ok(results)
    }

    fn files_from_archives(
        &self,
        filter: &[PackageRef],
        package_map: &PackageMap,
        interner: &Interner,
    ) -> Result<Vec<ArchiveResult>, PackageManagerError> {
        self.inner
            .files_from_archives(filter, package_map, interner)
    }

    fn prefer_files_from_archive(&self) -> bool {
        self.inner.prefer_files_from_archive()
    }

    fn cache_version(&self) -> u16 {
        self.inner.cache_version()
    }
}
