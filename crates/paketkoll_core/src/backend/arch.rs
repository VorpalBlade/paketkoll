//! The Arch Linux (and derivatives) backend

mod desc;
mod mtree;
mod pacman_conf;

use std::{
    collections::BTreeSet,
    io::BufReader,
    iter::once,
    path::{Path, PathBuf},
};

use super::{Files, FullBackend, Name, Packages};
use crate::{
    config::PackageFilter,
    types::{FileEntry, Package, PackageInterned, PackageRef},
};
use anyhow::Context;
use dashmap::DashSet;
use either::Either;
use paketkoll_types::intern::Interner;
use rayon::prelude::*;

const NAME: &str = "Arch Linux";

/// Arch Linux backend
#[derive(Debug)]
pub(crate) struct ArchLinux {
    pacman_config: pacman_conf::PacmanConfig,
    package_filter: &'static PackageFilter,
}

#[derive(Debug, Default)]
pub(crate) struct ArchLinuxBuilder {
    package_filter: Option<&'static PackageFilter>,
}

impl ArchLinuxBuilder {
    /// Load pacman config
    fn load_config(&mut self) -> anyhow::Result<pacman_conf::PacmanConfig> {
        log::debug!(target: "paketkoll_core::backend::arch", "Loading pacman config");
        let mut readable = BufReader::new(std::fs::File::open("/etc/pacman.conf")?);
        let pacman_config: pacman_conf::PacmanConfig =
            pacman_conf::PacmanConfig::new(&mut readable)?;

        if pacman_config.root != "/" {
            anyhow::bail!("Pacman root other than \"/\" not supported");
        }
        Ok(pacman_config)
    }

    pub fn package_filter(&mut self, filter: &'static PackageFilter) -> &mut Self {
        self.package_filter = Some(filter);
        self
    }

    pub fn build(mut self) -> anyhow::Result<ArchLinux> {
        let pacman_config = self.load_config().context("Failed to load pacman.conf")?;
        Ok(ArchLinux {
            // Impossible unwrap: We just loaded it
            pacman_config,
            package_filter: self
                .package_filter
                .unwrap_or_else(|| &PackageFilter::Everything),
        })
    }
}

impl Name for ArchLinux {
    fn name(&self) -> &'static str {
        NAME
    }
}

impl Files for ArchLinux {
    fn files(
        &self,
        interner: &paketkoll_types::intern::Interner,
    ) -> anyhow::Result<Vec<crate::types::FileEntry>> {
        let db_path: &Path = Path::new(&self.pacman_config.db_path);

        // Load packages
        log::debug!(target: "paketkoll_core::backend::arch", "Loading packages");
        let pkgs_and_paths = get_mtree_paths(db_path, interner, self.package_filter)?;

        // Load mtrees
        log::debug!(target: "paketkoll_core::backend::arch", "Loading mtrees");
        // Directories are duplicated across packages, we deduplicate them here
        let seen_directories = DashSet::new();
        // It is counter-intuitive, but we are faster if we collect into a vec here and start
        // over later on with a new parallel iteration. No idea why. (241 ms vs 264 ms according
        // to hyperfine on my machine, stdev < 4 ms in both cases).
        let results: anyhow::Result<Vec<FileEntry>> = pkgs_and_paths
            .into_par_iter()
            .flat_map_iter(|entry| match entry {
                Ok(PackageData {
                    name: pkg,
                    mtree_path,
                    backup_files,
                }) => {
                    let result = match mtree::extract_mtree(
                        pkg,
                        &mtree_path,
                        backup_files,
                        &seen_directories,
                    ) {
                        Ok(inner) => Either::Left(inner),
                        Err(err) => Either::Right(once(Err(err))),
                    };
                    result
                }
                Err(err) => Either::Right(once(Err(err))),
            })
            .collect();
        results
    }
}

impl Packages for ArchLinux {
    fn packages(
        &self,
        interner: &paketkoll_types::intern::Interner,
    ) -> anyhow::Result<Vec<PackageInterned>> {
        let db_root = Path::new(&self.pacman_config.db_path).join("local");
        let results: anyhow::Result<Vec<PackageInterned>> = std::fs::read_dir(db_root)
            .context("Failed to read pacman database directory")?
            .par_bridge()
            .filter_map(|entry| {
                let entry = entry.ok()?;
                load_pkg(&entry, interner)
                    .with_context(|| {
                        format!("Failed to load package data for {:?}", entry.file_name())
                    })
                    .transpose()
            })
            .collect();
        results
    }
}

impl FullBackend for ArchLinux {}

#[derive(Debug)]
struct PackageData {
    name: PackageRef,
    mtree_path: PathBuf,
    backup_files: BTreeSet<Vec<u8>>,
}

fn get_mtree_paths<'borrows>(
    db_path: &Path,
    interner: &'borrows Interner,
    package_filter: &'borrows PackageFilter,
) -> anyhow::Result<impl ParallelIterator<Item = anyhow::Result<PackageData>> + 'borrows> {
    let db_root = db_path.join("local");
    Ok(std::fs::read_dir(db_root)
        .context("Failed to read pacman database directory")?
        .par_bridge()
        .filter_map(|entry| {
            let entry = entry.ok()?;
            load_pkg_for_file_listing(&entry, interner, package_filter)
                .with_context(|| format!("Failed to load package data for {:?}", entry.file_name()))
                .transpose()
        }))
}

/// Process a single packaging, parsing the desc and files entries
#[inline]
fn load_pkg_for_file_listing(
    entry: &std::fs::DirEntry,
    interner: &Interner,
    package_filter: &PackageFilter,
) -> anyhow::Result<Option<PackageData>> {
    if !entry.file_type()?.is_dir() {
        return Ok(None);
    }
    let desc_path = entry.path().join("desc");
    let pkg_data = {
        let readable = BufReader::new(
            std::fs::File::open(&desc_path)
                .with_context(|| format!("Failed to open {desc_path:?}"))?,
        );
        Package::from_arch_linux_desc(readable, interner)?
    };

    // We need to read the desc file to know the package name, so it is only now
    // that we can decide if we should include it or not
    if !package_filter.should_include_interned(pkg_data.name, interner) {
        return Ok(None);
    }

    let files_path = entry.path().join("files");
    let backup_files = {
        let readable = BufReader::new(
            std::fs::File::open(&files_path)
                .with_context(|| format!("Failed to open {files_path:?}"))?,
        );
        desc::backup_files(readable)?
            .into_iter()
            .map(|e| format!("./{}", e).into_bytes())
            .collect()
    };
    Ok(Some(PackageData {
        name: pkg_data.name,
        mtree_path: entry.path().join("mtree"),
        backup_files,
    }))
}

/// Process a single packaging, parsing the desc and files entries
#[inline]
fn load_pkg(
    entry: &std::fs::DirEntry,
    interner: &Interner,
) -> anyhow::Result<Option<PackageInterned>> {
    if !entry.file_type()?.is_dir() {
        return Ok(None);
    }
    let desc_path = entry.path().join("desc");
    let pkg_data = {
        let readable = BufReader::new(
            std::fs::File::open(&desc_path)
                .with_context(|| format!("Failed to open {desc_path:?}"))?,
        );
        Package::from_arch_linux_desc(readable, interner)?
    };
    Ok(Some(pkg_data))
}
