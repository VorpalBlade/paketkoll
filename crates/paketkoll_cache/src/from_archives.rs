//! Wrapping backend that performs disk cache of original files queries

use crate::utils::format_package;
use ahash::AHashMap;
use cached::DiskCache;
use cached::IOCached;
use cached::stores::DiskCacheBuilder;
use compact_str::CompactString;
use eyre::OptionExt;
use eyre::WrapErr;
use paketkoll_types::backend::ArchiveQueryError;
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
use paketkoll_types::files::FileFlags;
use paketkoll_types::files::Properties;
use paketkoll_types::intern::Interner;
use paketkoll_types::intern::PackageRef;
use smallvec::SmallVec;
use std::fmt::Debug;
use std::fmt::Display;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    backend: &'static str,
    cache_version: u16,
    package: CompactString,
}

impl CacheKey {
    pub const fn new(backend: &'static str, cache_version: u16, package: CompactString) -> Self {
        Self {
            backend,
            cache_version,
            package,
        }
    }
}

impl Display for CacheKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}:{}:{}",
            self.backend, self.cache_version, self.package
        )
    }
}

/// A file entry from the package database
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct FileEntryCache {
    /// Package this file belongs to
    pub path: PathBuf,
    pub properties: Properties,
    pub flags: FileFlags,
}

impl FileEntryCache {
    pub fn into_full_entry(self, package: PackageRef, source: &'static str) -> FileEntry {
        FileEntry {
            package: Some(package),
            path: self.path,
            properties: self.properties,
            flags: self.flags,
            source,
            seen: Default::default(),
        }
    }
}

impl From<&FileEntry> for FileEntryCache {
    fn from(entry: &FileEntry) -> Self {
        Self {
            path: entry.path.clone(),
            properties: entry.properties.clone(),
            flags: entry.flags,
        }
    }
}

/// Wrapper to handle missing entries (for packages outside repositories)
#[derive(Debug, serde::Serialize, serde::Deserialize)]
enum FileEntryCacheWrapper {
    Cached(Vec<FileEntryCache>),
    Missing(CompactString, SmallVec<[CompactString; 4]>),
}

pub struct FromArchiveCache {
    inner: Box<dyn Files>,
    cache: DiskCache<CacheKey, FileEntryCacheWrapper>,
}

impl FromArchiveCache {
    pub fn from_path(inner: Box<dyn Files>, path: &Path) -> eyre::Result<Self> {
        let cache = DiskCacheBuilder::new("from_archives")
            .set_refresh(true)
            .set_lifespan(Duration::from_secs(60 * 60 * 24 * 15)) // Half a month
            .set_disk_directory(path)
            .build()?;
        Ok(Self { inner, cache })
    }
}

impl Debug for FromArchiveCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FromArchiveCache")
            .field("inner", &self.inner)
            .field("cache", &"DiskCache<OriginalFileQuery, Vec<u8>>")
            .finish()
    }
}

impl Name for FromArchiveCache {
    fn name(&self) -> &'static str {
        self.inner.name()
    }

    fn as_backend_enum(&self) -> Backend {
        self.inner.as_backend_enum()
    }
}

impl Files for FromArchiveCache {
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
    ) -> eyre::Result<dashmap::DashMap<PathBuf, Option<PackageRef>, ahash::RandomState>> {
        self.inner.owning_packages(paths, interner)
    }

    fn original_files(
        &self,
        queries: &[OriginalFileQuery],
        packages: &PackageMap,
        interner: &Interner,
    ) -> Result<OriginalFilesResult, OriginalFileError> {
        self.inner.original_files(queries, packages, interner)
    }

    fn files_from_archives(
        &self,
        filter: &[PackageRef],
        package_map: &PackageMap,
        interner: &Interner,
    ) -> Result<Vec<ArchiveResult>, PackageManagerError> {
        let inner_name = self.name();
        let cache_version = self.cache_version();
        let mut results = Vec::new();
        let mut uncached_queries = Vec::new();
        let mut cache_keys = AHashMap::new();

        for pkg_ref in filter {
            let pkg = package_map.get(pkg_ref).ok_or_eyre("Package not found")?;
            let cache_key = format_package(pkg, interner);
            let cache_key = CacheKey::new(inner_name, cache_version, cache_key);
            match self
                .cache
                .cache_get(&cache_key)
                .wrap_err("Cache query failed")?
            {
                Some(v) => match v {
                    FileEntryCacheWrapper::Cached(v) => {
                        results.push(Ok((
                            *pkg_ref,
                            v.into_iter()
                                .map(|e| e.into_full_entry(*pkg_ref, inner_name))
                                .collect(),
                        )));
                    }
                    FileEntryCacheWrapper::Missing(main_ref, refs) => {
                        let refs = refs
                            .into_iter()
                            .map(|e| PackageRef::get_or_intern(interner, &e))
                            .collect();
                        results.push(Err(ArchiveQueryError::PackageMissing {
                            query: PackageRef::get_or_intern(interner, main_ref),
                            alternates: refs,
                        }));
                    }
                },
                None => {
                    uncached_queries.push(*pkg_ref);
                    cache_keys.insert(pkg_ref, cache_key);
                }
            }
        }
        // Fetch uncached queries
        if !uncached_queries.is_empty() {
            let uncached_results =
                self.inner
                    .files_from_archives(&uncached_queries, package_map, interner)?;
            // Insert the uncached results into the cache and update the results
            for inner_result in uncached_results {
                match inner_result {
                    Ok((query, result)) => {
                        let cache_key = cache_keys.remove(&query).ok_or_else(|| {
                            eyre::eyre!("Cache key not found (archive): {query:?}")
                        })?;
                        self.cache
                            .cache_set(
                                cache_key.clone(),
                                FileEntryCacheWrapper::Cached(
                                    result.iter().map(Into::into).collect(),
                                ),
                            )
                            .wrap_err_with(|| {
                                format!(
                                    "Cache set failed: pkg={} cache_key={}",
                                    query.as_str(interner),
                                    cache_key
                                )
                            })?;
                        results.push(Ok((query, result)));
                    }
                    Err(ArchiveQueryError::PackageMissing { query, alternates }) => {
                        let pkgs: SmallVec<[CompactString; 4]> = alternates
                            .iter()
                            .map(|e| e.as_str(interner).into())
                            .collect();
                        let cache_key = cache_keys.remove(&query).ok_or_else(|| {
                            eyre::eyre!("Cache key not found (archive): {pkgs:?}")
                        })?;
                        self.cache
                            .cache_set(
                                cache_key.clone(),
                                FileEntryCacheWrapper::Missing(
                                    query.as_str(interner).into(),
                                    pkgs.clone(),
                                ),
                            )
                            .wrap_err_with(|| {
                                format!(
                                    "Negative cache set failed: pkgs={pkgs:?} \
                                     cache_key={cache_key}"
                                )
                            })?;
                        results.push(Err(ArchiveQueryError::PackageMissing { query, alternates }));
                    }

                    Err(e) => {
                        results.push(Err(e));
                    }
                }
            }
        }

        Ok(results)
    }

    fn prefer_files_from_archive(&self) -> bool {
        self.inner.prefer_files_from_archive()
    }

    fn cache_version(&self) -> u16 {
        self.inner.cache_version()
    }
}
