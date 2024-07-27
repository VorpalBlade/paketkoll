//! Utilities

use std::num::NonZeroUsize;

use anyhow::anyhow;
use clru::CLruCache;
use compact_str::CompactString;

use paketkoll_types::files::{Gid, Uid};

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
    pub(crate) fn lookup(&mut self, key: &IdKey<Uid, Gid>) -> anyhow::Result<CompactString> {
        match self.cache.get(key) {
            Some(v) => Ok(v.clone()),
            None => {
                // Resolve
                let name: CompactString = match key {
                    IdKey::User(uid) => {
                        nix::unistd::User::from_uid(uid.into())?
                            .ok_or_else(|| anyhow!("Failed to find user with ID {}", uid))?
                            .name
                    }
                    IdKey::Group(gid) => {
                        nix::unistd::Group::from_gid(gid.into())?
                            .ok_or_else(|| anyhow!("Failed to find group with ID {}", gid))?
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
    ) -> anyhow::Result<u32> {
        match self.cache.get(key) {
            Some(v) => Ok(*v),
            None => {
                // Resolve
                let id = match key {
                    IdKey::User(user) => nix::unistd::User::from_name(user.as_str())?
                        .ok_or_else(|| anyhow!("Failed to find user with ID {}", user))?
                        .uid
                        .as_raw(),
                    IdKey::Group(group) => nix::unistd::Group::from_name(group.as_str())?
                        .ok_or_else(|| anyhow!("Failed to find group with ID {}", group))?
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
