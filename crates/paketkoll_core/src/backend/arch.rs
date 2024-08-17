//! The Arch Linux (and derivatives) backend

use std::borrow::Cow;
use std::collections::BTreeSet;
use std::io::BufReader;
use std::iter::once;
use std::path::Path;
use std::path::PathBuf;

use ahash::AHashSet;
use anyhow::Context;
use bstr::ByteSlice;
use bstr::ByteVec;
use compact_str::format_compact;
use dashmap::DashMap;
use dashmap::DashSet;
use either::Either;
use paketkoll_types::backend::ArchiveQueryError;
use paketkoll_types::backend::ArchiveResult;
use paketkoll_types::backend::OriginalFileError;
use paketkoll_types::backend::OriginalFilesResult;
use paketkoll_types::backend::OwningPackagesResult;
use rayon::prelude::*;
use regex::RegexSet;

use paketkoll_types::backend::Files;
use paketkoll_types::backend::Name;
use paketkoll_types::backend::OriginalFileQuery;
use paketkoll_types::backend::PackageManagerError;
use paketkoll_types::backend::PackageMap;
use paketkoll_types::backend::Packages;
use paketkoll_types::files::FileEntry;
use paketkoll_types::intern::Interner;
use paketkoll_types::intern::PackageRef;
use paketkoll_types::package::PackageInterned;

use crate::utils::convert_archive_entries;
use crate::utils::extract_files;
use crate::utils::group_queries_by_pkg;
use crate::utils::locate_package_file;
use crate::utils::package_manager_transaction;

use super::FullBackend;
use super::PackageFilter;

mod desc;
mod mtree;
mod pacman_conf;

const NAME: &str = "Arch Linux";

/// Arch Linux backend
#[derive(Debug)]
pub(crate) struct ArchLinux {
    pacman_config: pacman_conf::PacmanConfig,
    package_filter: &'static PackageFilter,
    /// Mutex protecting calls to the package manager
    ///
    /// Yes it is strange with a mutex over (), but this doesn't protect an
    /// actual rust resource.
    pkgmgr_mutex: parking_lot::Mutex<()>,
}

#[derive(Debug, Default)]
pub(crate) struct ArchLinuxBuilder {
    package_filter: Option<&'static PackageFilter>,
}

impl ArchLinuxBuilder {
    /// Load pacman config
    fn load_config(&mut self) -> anyhow::Result<pacman_conf::PacmanConfig> {
        tracing::debug!("Loading pacman config");
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
            pkgmgr_mutex: parking_lot::Mutex::new(()),
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
    fn files(&self, interner: &Interner) -> anyhow::Result<Vec<FileEntry>> {
        let db_path: &Path = Path::new(&self.pacman_config.db_path);

        // Load packages
        tracing::debug!("Loading packages");
        let pkgs_and_paths = get_mtree_paths(db_path, interner, self.package_filter)?;

        // Load mtrees
        tracing::debug!("Loading mtrees");
        // Directories are duplicated across packages, we deduplicate them here
        let seen_directories = DashSet::new();
        // It is counter-intuitive, but we are faster if we collect into a vec here and
        // start over later on with a new parallel iteration. No idea why. (241
        // ms vs 264 ms according to hyperfine on my machine, stdev < 4 ms in
        // both cases).
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

    fn owning_packages(
        &self,
        paths: &AHashSet<&Path>,
        interner: &Interner,
    ) -> anyhow::Result<OwningPackagesResult> {
        // Optimise for speed, go directly into package cache and look for files that
        // contain the given string
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
                        tracing::error!("Failed to parse package data: {e}");
                    }
                }
            });

        Ok(file_to_package)
    }

    fn original_files(
        &self,
        queries: &[OriginalFileQuery],
        packages: &PackageMap,
        interner: &Interner,
    ) -> Result<OriginalFilesResult, OriginalFileError> {
        let queries_by_pkg = group_queries_by_pkg(queries);

        let mut results = OriginalFilesResult::new();

        // List of directories to search for the package
        let dir_candidates = smallvec::smallvec_inline![self.pacman_config.cache_dir.as_str()];

        for (pkg, queries) in queries_by_pkg {
            // We may not have exact package name, try to figure this out:
            let package_match = guess_pkg_file_name(interner, pkg, packages);

            let package_path =
                locate_package_file(dir_candidates.as_slice(), &package_match, pkg, |pkg| {
                    let _guard = self.pkgmgr_mutex.lock();
                    download_arch_pkg(pkg)
                })?;
            // Error if we couldn't find the package
            let package_path = package_path
                .ok_or_else(|| OriginalFileError::PackageNotFound(format_compact!("{pkg}")))?;

            // The package is a .tar.zst
            let package_file =
                std::fs::File::open(&package_path).context("Failed to open archive")?;
            let decompressed =
                zstd::Decoder::new(package_file).context("Failed to create zstd decompressor")?;
            let archive = tar::Archive::new(decompressed);

            // Now, lets extract the requested files from the package
            extract_files(archive, &queries, &mut results, pkg, |path| {
                Some(format_compact!("/{path}"))
            })?;
        }

        Ok(results)
    }

    fn files_from_archives(
        &self,
        filter: &[PackageRef],
        package_map: &PackageMap,
        interner: &Interner,
    ) -> Result<Vec<ArchiveResult>, PackageManagerError> {
        tracing::info!(
            "Finding archives for {} packages (may take a while)",
            filter.len()
        );
        let archives = self.iterate_pkg_archives(filter, package_map, interner);

        tracing::info!(
            "Loading files from {} archives (may take a while)",
            filter.len()
        );
        let results: Vec<_> = archives
            .par_bridge()
            .map(|value| {
                value.and_then(|(pkg_ref, path)| Ok((pkg_ref, archive_to_entries(pkg_ref, &path)?)))
            })
            .collect();
        Ok(results)
    }
}

impl ArchLinux {
    /// Find all pkg archives for the given packages
    fn iterate_pkg_archives<'inputs>(
        &'inputs self,
        filter: &'inputs [PackageRef],
        packages: &'inputs PackageMap,
        interner: &'inputs Interner,
    ) -> impl Iterator<Item = Result<(PackageRef, PathBuf), ArchiveQueryError>> + 'inputs {
        let package_paths = filter.iter().map(|pkg_ref| {
            let pkg = packages
                .get(pkg_ref)
                .context("Failed to find package in package map")?;
            let name = pkg.name.to_str(interner);
            // Get the full file name
            let filename = format_pkg_filename(interner, pkg);

            let package_path =
                locate_package_file(&[&self.pacman_config.cache_dir], &filename, name, |pkg| {
                    let _guard = self.pkgmgr_mutex.lock();
                    download_arch_pkg(pkg)
                })?;
            // Error if we couldn't find the package
            let package_path = package_path.ok_or_else(|| ArchiveQueryError::PackageMissing {
                query: *pkg_ref,
                alternates: smallvec::smallvec![*pkg_ref],
            })?;
            Ok((*pkg_ref, package_path))
        });

        package_paths
    }
}

/// Convert deb archives to file entries
fn archive_to_entries(pkg_ref: PackageRef, pkg_file: &Path) -> anyhow::Result<Vec<FileEntry>> {
    // The package is a .tar.zst
    let package_file = std::fs::File::open(pkg_file)?;
    let decompressed = zstd::Decoder::new(package_file)?;
    let archive = tar::Archive::new(decompressed);

    // Now, lets extract the requested files from the package
    convert_archive_entries(archive, pkg_ref, NAME, |path| {
        let path = path.as_os_str().as_encoded_bytes();
        if SPECIAL_ARCHIVE_FILES.contains(path) {
            None
        } else {
            let path = path.trim_end_with(|ch| ch == '/');
            let path = bstr::concat([b"/", path]);
            Some(Cow::Owned(path.into_path_buf().expect("Invalid path")))
        }
    })
}

/// Files to ignore when reading archives
const SPECIAL_ARCHIVE_FILES: phf::Set<&'static [u8]> = phf::phf_set! {
    b".BUILDINFO",
    b".CHANGELOG",
    b".PKGINFO",
    b".INSTALL",
    b".MTREE",
};

fn format_pkg_filename(interner: &Interner, package: &PackageInterned) -> String {
    format!(
        "{}-{}-{}.pkg.tar.zst",
        package.name.to_str(interner),
        package.version,
        package
            .architecture
            .map(|e| e.to_str(interner))
            .unwrap_or("*")
    )
}

fn guess_pkg_file_name(interner: &Interner, pkg: &str, packages: &PackageMap) -> String {
    let package_match = if let Some(pkgref) = interner.get(pkg) {
        // Yay, it is probably installed, we know what to look for
        if let Some(package) = packages.get(&PackageRef::new(pkgref)) {
            format_pkg_filename(interner, package)
        } else {
            format!("{}-*-*.pkg.tar.zst", pkg)
        }
    } else {
        format!("{}-*-*.pkg.tar.zst", pkg)
    };
    package_match
}

fn find_files(
    entry: &std::fs::DirEntry,
    interner: &Interner,
    re: &RegexSet,
    paths: &[String],
    output: &OwningPackagesResult,
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
    fn packages(&self, interner: &Interner) -> anyhow::Result<Vec<PackageInterned>> {
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
    ) -> Result<(), PackageManagerError> {
        let _guard = self.pkgmgr_mutex.lock();
        if !install.is_empty() {
            package_manager_transaction(
                "pacman",
                &["-S"],
                install,
                (!ask_confirmation).then_some("--noconfirm"),
            )
            .context("Failed to install with pacman")?;
        }
        if !uninstall.is_empty() {
            package_manager_transaction(
                "pacman",
                &["-R"],
                uninstall,
                (!ask_confirmation).then_some("--noconfirm"),
            )
            .context("Failed to uninstall with pacman")?;
        }
        Ok(())
    }

    fn mark(&self, dependencies: &[&str], manual: &[&str]) -> Result<(), PackageManagerError> {
        let _guard = self.pkgmgr_mutex.lock();
        if !dependencies.is_empty() {
            package_manager_transaction("pacman", &["-D", "--asdeps"], dependencies, None)
                .context("Failed to mark dependencies with pacman")?;
        }
        if !manual.is_empty() {
            package_manager_transaction("pacman", &["-D", "--asexplicit"], manual, None)
                .context("Failed to mark manual with pacman")?;
        }
        Ok(())
    }

    fn remove_unused(&self, ask_confirmation: bool) -> Result<(), PackageManagerError> {
        let _guard = self.pkgmgr_mutex.lock();
        let mut query_cmd = std::process::Command::new("pacman");
        query_cmd.args(["-Qttdq"]);

        let mut run_query = || -> anyhow::Result<Option<String>> {
            let query_output = query_cmd
                .output()
                .with_context(|| "Failed to execute pacman -Qdtq")?;
            let out = String::from_utf8(query_output.stdout)
                .with_context(|| "Failed to parse pacman -Qdtq output as UTF-8")?;
            if out.is_empty() {
                Ok(None)
            } else {
                Ok(Some(out))
            }
        };

        while let Some(packages) = run_query()? {
            let packages = packages.lines().collect::<Vec<_>>();
            package_manager_transaction(
                "pacman",
                &["-R"],
                &packages,
                (!ask_confirmation).then_some("--noconfirm"),
            )
            .context("Failed to remove unused packages with pacman")?;
        }

        Ok(())
    }
}

// To download to cache: pacman -Sw packagename
// /var/cache/pacman/pkg/packagename-version-arch.pkg.tar.zst
// If foreign, also maybe look in other locations based on what aconfmgr does
// arch: any, x86_64
// Epoch separator is :

fn download_arch_pkg(pkg: &str) -> anyhow::Result<()> {
    let status = std::process::Command::new("pacman")
        .args(["-Sw", "--noconfirm", pkg])
        .status()?;
    if !status.success() {
        tracing::warn!("Failed to download package for {pkg}");
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
