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

use super::{FullBackend, PackageFilter};
use crate::utils::{
    extract_files, group_queries_by_pkg, locate_package_file, package_manager_transaction,
};
use ahash::AHashSet;
use anyhow::Context;
use compact_str::format_compact;
use dashmap::{DashMap, DashSet};
use either::Either;
use paketkoll_types::backend::{Files, Name, OriginalFileQuery, PackageMap, Packages};
use paketkoll_types::{files::FileEntry, intern::PackageRef};
use paketkoll_types::{intern::Interner, package::PackageInterned};
use rayon::prelude::*;
use regex::RegexSet;

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

    fn as_backend_enum(&self) -> paketkoll_types::backend::Backend {
        paketkoll_types::backend::Backend::Pacman
    }
}

impl Files for ArchLinux {
    fn files(
        &self,
        interner: &paketkoll_types::intern::Interner,
    ) -> anyhow::Result<Vec<FileEntry>> {
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

    fn original_files(
        &self,
        queries: &[OriginalFileQuery],
        packages: &PackageMap,
        interner: &Interner,
    ) -> anyhow::Result<ahash::AHashMap<OriginalFileQuery, Vec<u8>>> {
        let queries_by_pkg = group_queries_by_pkg(queries);

        let mut results = ahash::AHashMap::new();

        // List of directories to search for the package
        let dir_candidates = smallvec::smallvec_inline![self.pacman_config.cache_dir.as_str()];

        for (pkg, queries) in queries_by_pkg {
            // We may not have exact package name, try to figure this out:
            let package_match = if let Some(pkgref) = interner.get(pkg) {
                // Yay, it is probably installed, we know what to look for
                if let Some(package) = packages.get(&PackageRef::new(pkgref)) {
                    format!(
                        "{}-{}-{}.pkg.tar.zst",
                        pkg,
                        package.version,
                        package
                            .architecture
                            .map(|e| e.to_str(interner))
                            .unwrap_or("*")
                    )
                } else {
                    format!("{}-*-*.pkg.tar.zst", pkg)
                }
            } else {
                format!("{}-*-*.pkg.tar.zst", pkg)
            };

            let package_path = locate_package_file(
                dir_candidates.as_slice(),
                &package_match,
                pkg,
                download_arch_pkg,
            )?;
            // Error if we couldn't find the package
            let package_path = package_path.ok_or_else(|| {
                anyhow::anyhow!("Failed to find or download package file for {pkg}")
            })?;

            // The package is a .tar.zst
            let package_file = std::fs::File::open(&package_path)?;
            let decompressed = zstd::Decoder::new(package_file)?;
            let archive = tar::Archive::new(decompressed);

            // Now, lets extract the requested files from the package
            extract_files(archive, &queries, &mut results, pkg, |path| {
                format_compact!("/{path}")
            })?;
        }

        Ok(results)
    }

    fn owning_package(
        &self,
        paths: &AHashSet<PathBuf>,
        interner: &Interner,
    ) -> anyhow::Result<DashMap<PathBuf, Option<PackageRef>, ahash::RandomState>> {
        // Optimise for speed, go directly into package cache and look for files that contain the given string
        let file_to_package = DashMap::with_hasher(ahash::RandomState::new());
        let db_root = PathBuf::from(self.pacman_config.db_path.as_str()).join("local");

        let paths: Vec<String> = paths
            .iter()
            .map(|e| {
                let e = e.to_string_lossy();
                let e = e.as_ref();
                format!("\n{}\n", e.strip_prefix('/').unwrap_or(e))
            })
            .collect();
        let paths = paths.as_slice();
        let re = RegexSet::new(paths)?;

        std::fs::read_dir(db_root)
            .context("Failed to read pacman database directory")?
            .par_bridge()
            .for_each(|entry| {
                if let Ok(entry) = entry {
                    if let Err(e) = find_files(&entry, interner, &re, paths, &file_to_package) {
                        log::error!("Failed to parse package data: {e}");
                    }
                }
            });

        Ok(file_to_package)
    }
}

fn find_files(
    entry: &std::fs::DirEntry,
    interner: &Interner,
    re: &RegexSet,
    paths: &[String],
    output: &DashMap<PathBuf, Option<PackageRef>, ahash::RandomState>,
) -> anyhow::Result<()> {
    if !entry.file_type()?.is_dir() {
        return Ok(());
    };
    let files_path = entry.path().join("files");
    let contents = std::fs::read_to_string(&files_path)
        .with_context(|| format!("Failed to read {files_path:?}"))?;
    let matches = re.matches(&contents);
    if matches.matched_any() {
        let desc_path = entry.path().join("desc");
        let pkg_data = {
            let readable = BufReader::new(
                std::fs::File::open(&desc_path)
                    .with_context(|| format!("Failed to open {desc_path:?}"))?,
            );
            desc::from_arch_linux_desc(readable, interner)?
        };

        for m in matches {
            output.insert(
                format!("/{}", paths[m].as_str().trim()).into(),
                Some(pkg_data.name),
            );
        }
    }
    Ok(())
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

    fn transact(
        &self,
        install: &[&str],
        uninstall: &[&str],
        ask_confirmation: bool,
    ) -> anyhow::Result<()> {
        if !install.is_empty() {
            package_manager_transaction(
                "pacman",
                "-S",
                install,
                ask_confirmation.then_some("--noconfirm"),
            )
            .context("Failed to install with pacman")?;
        }
        if !uninstall.is_empty() {
            package_manager_transaction(
                "pacman",
                "-R",
                uninstall,
                ask_confirmation.then_some("--noconfirm"),
            )
            .context("Failed to uninstall with pacman")?;
        }
        Ok(())
    }
}

// To download to cache: pacman -Sw packagename
// /var/cache/pacman/pkg/packagename-version-arch.pkg.tar.zst
// If foreign, also maybe look in other locations based on what aconfmgr does
// arch: any, x86_64
// Epoch separator is :

fn download_arch_pkg(pkg: &str) -> Result<(), anyhow::Error> {
    let status = std::process::Command::new("pacman")
        .args(["-Sw", "--noconfirm", pkg])
        .status()?;
    if !status.success() {
        log::warn!(target: "paketkoll_core::backend::arch", "Failed to download package for {pkg}");
    };
    Ok(())
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
        desc::from_arch_linux_desc(readable, interner)?
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
        desc::from_arch_linux_desc(readable, interner)?
    };
    Ok(Some(pkg_data))
}
