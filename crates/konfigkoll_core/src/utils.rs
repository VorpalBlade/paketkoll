//! Utilities

use clru::CLruCache;
use compact_str::CompactString;
use eyre::eyre;
use eyre::Context;
use konfigkoll_types::FsInstruction;
use paketkoll_types::backend::Files;
use paketkoll_types::backend::OriginalFileQuery;
use paketkoll_types::backend::PackageMap;
use paketkoll_types::backend::PackageMapMap;
use paketkoll_types::files::Gid;
use paketkoll_types::files::Uid;
use paketkoll_types::intern::Interner;
use std::num::NonZeroUsize;
use std::sync::Arc;

/// UID/GID to name resolver / cache
#[derive(Debug)]
pub(crate) struct IdResolveCache<Key, Value> {
    cache: CLruCache<Key, Value, ahash::RandomState>,
}

impl<Key, Value> IdResolveCache<Key, Value>
where
    Key: PartialEq + Eq + std::hash::Hash,
{
    /// Create a new instance
    pub(crate) fn new() -> Self {
        Self {
            cache: CLruCache::with_hasher(
                NonZeroUsize::new(100).expect("Compile time constant"),
                ahash::RandomState::new(),
            ),
        }
    }
}

impl<Key, Value> Default for IdResolveCache<Key, Value>
where
    Key: PartialEq + Eq + std::hash::Hash,
{
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) type IdKeyId = IdKey<Uid, Gid>;
pub(crate) type IdKeyName = IdKey<CompactString, CompactString>;

pub(crate) type NumericToNameResolveCache = IdResolveCache<IdKeyId, CompactString>;
pub(crate) type NameToNumericResolveCache = IdResolveCache<IdKeyName, u32>;

impl IdResolveCache<IdKey<Uid, Gid>, CompactString> {
    /// Lookup a UID/GID (resolving and caching if necessary)
    pub(crate) fn lookup(&mut self, key: &IdKey<Uid, Gid>) -> eyre::Result<CompactString> {
        match self.cache.get(key) {
            Some(v) => Ok(v.clone()),
            None => {
                // Resolve
                let name: CompactString = match key {
                    IdKey::User(uid) => {
                        nix::unistd::User::from_uid(uid.into())?
                            .ok_or_else(|| eyre!("Failed to find user with ID {}", uid))?
                            .name
                    }
                    IdKey::Group(gid) => {
                        nix::unistd::Group::from_gid(gid.into())?
                            .ok_or_else(|| eyre!("Failed to find group with ID {}", gid))?
                            .name
                    }
                }
                .into();
                self.cache.put(*key, name.clone());
                Ok(name)
            }
        }
    }
}

impl IdResolveCache<IdKey<CompactString, CompactString>, u32> {
    /// Lookup a UID/GID (resolving and caching if necessary)
    pub(crate) fn lookup(
        &mut self,
        key: &IdKey<CompactString, CompactString>,
    ) -> eyre::Result<u32> {
        match self.cache.get(key) {
            Some(v) => Ok(*v),
            None => {
                // Resolve
                let id = match key {
                    IdKey::User(user) => nix::unistd::User::from_name(user.as_str())?
                        .ok_or_else(|| eyre!("Failed to find user with ID {}", user))?
                        .uid
                        .as_raw(),
                    IdKey::Group(group) => nix::unistd::Group::from_name(group.as_str())?
                        .ok_or_else(|| eyre!("Failed to find group with ID {}", group))?
                        .gid
                        .as_raw(),
                };
                self.cache.put(key.clone(), id);
                Ok(id)
            }
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum IdKey<UserKey, GroupKey>
where
    UserKey: Clone + std::fmt::Debug + PartialEq + Eq + std::hash::Hash,
    GroupKey: Clone + std::fmt::Debug + PartialEq + Eq + std::hash::Hash,
{
    User(UserKey),
    Group(GroupKey),
}

/// Resolve the original contents of a file given a file backend and instruction
pub(crate) fn original_file_contents(
    file_backend: &dyn Files,
    interner: &Interner,
    instr: &FsInstruction,
    pkg_map: &PackageMap,
) -> Result<Vec<u8>, eyre::Error> {
    // Get package owning the file
    let owners = file_backend
        .owning_packages(&[instr.path.as_std_path()].into(), interner)
        .wrap_err_with(|| format!("Failed to find owner for {}", instr.path))?;
    let package = owners
        .get(instr.path.as_std_path())
        .ok_or_else(|| eyre::eyre!("Failed to find owner for {}", instr.path))?
        .ok_or_else(|| eyre::eyre!("No owner for {}", instr.path))?;
    let package = package.as_str(interner);
    // Create query
    let queries = [OriginalFileQuery {
        package: package.into(),
        path: instr.path.as_str().into(),
    }];
    // Get file contents
    let mut original_contents = file_backend.original_files(&queries, pkg_map, interner)?;
    let original_contents = original_contents
        .remove(&queries[0])
        .ok_or_else(|| eyre::eyre!("No original contents for {:?}", queries[0]))?;
    Ok(original_contents)
}

/// Helper to get the package map for the file backend
pub fn pkg_backend_for_files(
    package_maps: &PackageMapMap,
    file_backend: &dyn Files,
) -> Result<Arc<PackageMap>, eyre::Error> {
    let pkg_map = package_maps
        .get(&file_backend.as_backend_enum())
        .ok_or_else(|| {
            eyre::eyre!(
                "No package map for file backend {:?}",
                file_backend.as_backend_enum()
            )
        })?
        .clone();
    Ok(pkg_map)
}
