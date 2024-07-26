//! Wrapping backend that performs disk cache of original files queries

use std::fmt::Debug;
use std::fmt::Display;
use std::path::Path;

use ahash::AHashMap;
use anyhow::Context;
use cached::stores::DiskCacheBuilder;
use cached::DiskCache;
use cached::IOCached;
use compact_str::format_compact;
use compact_str::CompactString;

use paketkoll_types::{
    backend::{Backend, Files, Name, OriginalFileQuery, PackageMap},
    intern::{Interner, PackageRef},
};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct CacheKey {
    backend: &'static str,
    package: CompactString,
    path: CompactString,
}

impl CacheKey {
    pub fn new(backend: &'static str, package: CompactString, path: CompactString) -> Self {
        Self {
            backend,
            package,
            path,
        }
    }
}

impl Display for CacheKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}:{}:{}", self.backend, self.package, self.path)
    }
}

pub struct OriginalFilesCache {
    inner: Box<dyn Files>,
    cache: DiskCache<CacheKey, Vec<u8>>,
}

impl OriginalFilesCache {
    pub fn from_path(inner: Box<dyn Files>, path: &Path) -> anyhow::Result<Self> {
        let cache = DiskCacheBuilder::new(inner.name())
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
    fn files(&self, interner: &Interner) -> anyhow::Result<Vec<paketkoll_types::files::FileEntry>> {
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
        // Build up lists of cached and uncached queries
        let mut results = AHashMap::new();
        let mut uncached_queries = Vec::new();
        let mut cache_keys = AHashMap::new();
        let inner_name = self.name();
        for query in queries.iter() {
            // Resolve exact version and ID of packages from the package map
            let cache_key = match packages.get(&PackageRef::get_or_intern(interner, &query.package))
            {
                Some(p) => {
                    let ids = p.ids.iter().map(|v| v.to_str(interner));
                    let ids = ids.collect::<Vec<_>>().join("#");
                    format_compact!(
                        "{}:{}:{}:{}",
                        query.package,
                        p.architecture
                            .map(|v| v.to_str(interner))
                            .unwrap_or_default(),
                        p.version,
                        ids
                    )
                }
                None => {
                    tracing::warn!("Package not found: {}", query.package);
                    uncached_queries.push(query.clone());
                    continue;
                }
            };
            let cache_key = CacheKey::new(inner_name, cache_key, query.path.clone());
            match self.cache.cache_get(&cache_key)? {
                Some(v) => {
                    results.insert(query.clone(), v);
                }
                None => {
                    uncached_queries.push(query.clone());
                    cache_keys.insert(query.clone(), cache_key);
                }
            }
        }
        // Fetch uncached queries
        let uncached_results = self
            .inner
            .original_files(&uncached_queries, packages, interner)?;

        // Insert the uncached results into the cache and update the results
        for (query, result) in uncached_results.into_iter() {
            let cache_key = cache_keys.get(&query).context("Cache key not found")?;
            self.cache.cache_set(cache_key.clone(), result.clone())?;
            results.insert(query, result);
        }

        Ok(results)
    }
}
