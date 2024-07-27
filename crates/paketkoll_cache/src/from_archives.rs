//! Wrapping backend that performs disk cache of original files queries

use std::fmt::Debug;
use std::fmt::Display;
use std::path::Path;
use std::path::PathBuf;

use ahash::AHashMap;
use anyhow::Context;
use cached::stores::DiskCacheBuilder;
use cached::DiskCache;
use cached::IOCached;
use compact_str::CompactString;

use paketkoll_types::backend::Backend;
use paketkoll_types::backend::Files;
use paketkoll_types::backend::Name;
use paketkoll_types::backend::OriginalFileQuery;
use paketkoll_types::backend::PackageManagerError;
use paketkoll_types::backend::PackageMap;
use paketkoll_types::files::FileEntry;
use paketkoll_types::files::FileFlags;
use paketkoll_types::files::Properties;
use paketkoll_types::intern::Interner;
use paketkoll_types::intern::PackageRef;

use crate::utils::format_package;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    backend: &'static str,
    package: CompactString,
}

impl CacheKey {
    pub fn new(backend: &'static str, package: CompactString) -> Self {
        Self { backend, package }
    }
}

impl Display for CacheKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}", self.backend, self.package)
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

pub struct FromArchiveCache {
    inner: Box<dyn Files>,
    cache: DiskCache<CacheKey, Vec<FileEntryCache>>,
}

impl FromArchiveCache {
    pub fn from_path(inner: Box<dyn Files>, path: &Path) -> anyhow::Result<Self> {
        let cache = DiskCacheBuilder::new("from_archives")
            .set_refresh(true)
            .set_lifespan(60 * 60 * 24 * 15) // Half a month
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
    fn files(&self, interner: &Interner) -> anyhow::Result<Vec<FileEntry>> {
        self.inner.files(interner)
    }

    fn may_need_canonicalization(&self) -> bool {
        self.inner.may_need_canonicalization()
    }

    fn owning_packages(
        &self,
        paths: &ahash::AHashSet<&Path>,
        interner: &Interner,
    ) -> anyhow::Result<dashmap::DashMap<std::path::PathBuf, Option<PackageRef>, ahash::RandomState>>
    {
        self.inner.owning_packages(paths, interner)
    }

    fn original_files(
        &self,
        queries: &[OriginalFileQuery],
        packages: &PackageMap,
        interner: &Interner,
    ) -> anyhow::Result<AHashMap<OriginalFileQuery, Vec<u8>>> {
        self.inner.original_files(queries, packages, interner)
    }

    fn files_from_archives(
        &self,
        filter: &[PackageRef],
        package_map: &PackageMap,
        interner: &Interner,
    ) -> Result<Vec<(PackageRef, Vec<FileEntry>)>, PackageManagerError> {
        let inner_name = self.name();
        let mut results = Vec::new();
        let mut uncached_queries = Vec::new();
        let mut cache_keys = AHashMap::new();

        for pkg_ref in filter {
            let pkg = package_map.get(pkg_ref).context("Package not found")?;
            let cache_key = format_package(pkg, interner);
            let cache_key = CacheKey::new(inner_name, cache_key);
            match self
                .cache
                .cache_get(&cache_key)
                .context("Cache query failed")?
            {
                Some(v) => {
                    results.push((
                        *pkg_ref,
                        v.into_iter()
                            .map(|e| e.into_full_entry(*pkg_ref, inner_name))
                            .collect(),
                    ));
                }
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
            for (query, result) in uncached_results.into_iter() {
                let cache_key = cache_keys.remove(&query).context("Cache key not found")?;
                self.cache
                    .cache_set(cache_key.clone(), result.iter().map(Into::into).collect())
                    .with_context(|| {
                        format!(
                            "Cache set failed: pkg={} cache_key={}",
                            query.to_str(interner),
                            cache_key
                        )
                    })?;
                results.push((query, result));
            }
        }

        Ok(results)
    }

    fn prefer_files_from_archive(&self) -> bool {
        self.inner.prefer_files_from_archive()
    }
}
