//! Backend for Debian and derivatives
use super::FullBackend;
use crate::backend::PackageFilter;
use crate::utils::convert_archive_entries;
use crate::utils::extract_files;
use crate::utils::group_queries_by_pkg;
use crate::utils::locate_package_file;
use crate::utils::missing_packages;
use crate::utils::package_manager_transaction;
use crate::utils::CompressionFormat;
use crate::utils::PackageQuery;
use bstr::ByteSlice;
use bstr::ByteVec;
use compact_str::format_compact;
use compact_str::CompactString;
use dashmap::DashMap;
use eyre::WrapErr;
use paketkoll_types::backend::ArchiveQueryError;
use paketkoll_types::backend::ArchiveResult;
use paketkoll_types::backend::Files;
use paketkoll_types::backend::Name;
use paketkoll_types::backend::OriginalFileError;
use paketkoll_types::backend::OriginalFileQuery;
use paketkoll_types::backend::OriginalFilesResult;
use paketkoll_types::backend::OwningPackagesResult;
use paketkoll_types::backend::PackageManagerError;
use paketkoll_types::backend::PackageMap;
use paketkoll_types::backend::Packages;
use paketkoll_types::files::FileEntry;
use paketkoll_types::files::Properties;
use paketkoll_types::intern::ArchitectureRef;
use paketkoll_types::intern::Interner;
use paketkoll_types::intern::PackageRef;
use paketkoll_types::package::PackageInterned;
use rayon::prelude::*;
use regex::RegexSet;
use std::borrow::Cow;
use std::fs::DirEntry;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;
use std::path::PathBuf;

mod divert;
mod parsers;

// Each package has a set of files in DB_PATH:
// *.list (all installed paths, one per line, including directories)
// *.md5sums (md5sum<space>path, one per line for all regular files)
// *.conffiles (may not exist, one file name per line)
// There are other files we don't care about (.symbols, .postinst, ...)
//
// Special files: /var/lib/dpkg/info/format (contains "1")
//
// Config files have no checksums in md5sums, so we need to parse
// /var/lib/dpkg/status for that.

const DB_PATH: &str = "/var/lib/dpkg/info";
const STATUS_PATH: &str = "/var/lib/dpkg/status";
const EXTENDED_STATUS_PATH: &str = "/var/lib/apt/extended_states";
const CACHE_PATH: &str = "/var/cache/apt/archives";
const NAME: &str = "Debian";

#[derive(Debug)]
pub(crate) struct Debian {
    package_filter: &'static PackageFilter,
    primary_architecture: ArchitectureRef,
    /// Mutex protecting calls to the package manager
    ///
    /// Yes it is strange with a mutex over (), but this doesn't protect an
    /// actual rust resource.
    pkgmgr_mutex: parking_lot::Mutex<()>,
}

#[derive(Debug, Default)]
pub(crate) struct DebianBuilder {
    package_filter: Option<&'static PackageFilter>,
}

impl DebianBuilder {
    pub fn package_filter(&mut self, filter: &'static PackageFilter) -> &mut Self {
        self.package_filter = Some(filter);
        self
    }

    pub fn build(self, interner: &Interner) -> Debian {
        let arch = std::process::Command::new("dpkg")
            .args(["--print-architecture"])
            .output()
            .expect("Failed to get primary architecture")
            .stdout;
        let arch_str = arch.trim();
        let primary_architecture =
            ArchitectureRef::get_or_intern(interner, arch_str.to_str_lossy().as_ref());
        Debian {
            package_filter: self
                .package_filter
                .unwrap_or_else(|| &PackageFilter::Everything),
            primary_architecture,
            pkgmgr_mutex: parking_lot::Mutex::new(()),
        }
    }
}

impl Name for Debian {
    fn name(&self) -> &'static str {
        NAME
    }

    fn as_backend_enum(&self) -> paketkoll_types::backend::Backend {
        paketkoll_types::backend::Backend::Apt
    }
}

impl Files for Debian {
    fn files(&self, interner: &Interner) -> eyre::Result<Vec<FileEntry>> {
        tracing::debug!("Loading packages");
        let packages_files: Vec<_> = get_package_files(interner)?.collect();

        // Handle diversions: (parse output of dpkg-divert --list)
        tracing::debug!("Loading diversions");
        let diversions =
            divert::get_diversions(interner).wrap_err("Failed to get dpkg diversions")?;

        // Load config files.
        tracing::debug!("Loading status to get config files");
        let (config_files, _) = {
            let mut status = BufReader::new(File::open(STATUS_PATH)?);
            parsers::parse_status(interner, &mut status, self.primary_architecture)
        }
        .context(format!("Failed to parse {STATUS_PATH}"))?;

        tracing::debug!("Merging packages files into one map");
        let merged = DashMap::with_hasher(ahash::RandomState::new());
        packages_files.into_par_iter().for_each(|files| {
            merge_deb_fileentries(&merged, files, &diversions);
        });

        // The config files must be merged into the results
        tracing::debug!("Merging config files");
        merge_deb_fileentries(&merged, config_files, &diversions);

        // For Debian we apply the filter here at the end, since multiple steps
        // needs filter otherwise. The fast path is not filtering.
        match self.package_filter {
            PackageFilter::Everything => (),
            PackageFilter::FilterFunction(_) => {
                merged.retain(|_, file| match file.package {
                    Some(pkg) => self.package_filter.should_include_interned(pkg, interner),
                    None => true,
                });
            }
        }

        // Finally extract just the file entries
        Ok(merged.into_iter().map(|(_, v)| v).collect())
    }

    fn may_need_canonicalization(&self) -> bool {
        true
    }

    fn owning_packages(
        &self,
        paths: &ahash::AHashSet<&Path>,
        interner: &Interner,
    ) -> eyre::Result<OwningPackagesResult> {
        // Optimise for speed, go directly into package cache and look for files that
        // contain the given string
        let file_to_package = DashMap::with_hasher(ahash::RandomState::new());
        let db_root = PathBuf::from(DB_PATH);

        let paths: Vec<String> = paths
            .iter()
            .map(|e| {
                let e = e.to_string_lossy();
                let e = e.as_ref();
                format!("\n{e}\n")
            })
            .collect();
        let paths = paths.as_slice();
        let re = RegexSet::new(paths)?;

        std::fs::read_dir(db_root)
            .wrap_err("Failed to read dpkg database directory")?
            .par_bridge()
            .for_each(|entry| {
                if let Ok(entry) = entry {
                    if entry.file_name().as_encoded_bytes().ends_with(b".list") {
                        if let Err(e) =
                            is_file_match(&entry.path(), interner, &re, paths, &file_to_package)
                        {
                            tracing::error!("Failed to parse package data: {e}");
                        }
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
        let dir_candidates = smallvec::smallvec_inline![CACHE_PATH];

        for (pkg, queries) in queries_by_pkg {
            // We may not have exact package name, try to figure this out:
            let package_match = guess_deb_file_name(interner, pkg, packages);

            let package_path =
                locate_package_file(dir_candidates.as_slice(), &package_match, pkg, |pkg| {
                    let _guard = self.pkgmgr_mutex.lock();
                    download_deb(pkg)
                })?;
            // Error if we couldn't find the package
            let package_path = package_path
                .ok_or_else(|| OriginalFileError::PackageNotFound(format_compact!("{pkg}")))?;

            // The package is a .deb, which is actually an ar archive
            let package_file = File::open(&package_path).wrap_err("Failed to open archive")?;
            let mut archive = ar::Archive::new(package_file);

            // We want the data.tar.xz file (or other compression scheme)
            while let Some(entry) = archive.next_entry() {
                let mut entry = entry.wrap_err("Failed to process entry in .deb (ar level)")?;
                if entry.header().identifier().starts_with(b"data.tar") {
                    let extension: CompactString = std::str::from_utf8(entry.header().identifier())
                        .wrap_err("Failed to parse file entry (ar level) as UTF-8")?
                        .split('.')
                        .last()
                        .ok_or_else(|| eyre::eyre!("No file extension found"))?
                        .into();
                    let mut decompressed =
                        CompressionFormat::from_extension(&extension, &mut entry)?;
                    let archive = tar::Archive::new(&mut decompressed);
                    // Now, lets extract the requested files from the package
                    extract_files(archive, &queries, &mut results, pkg, |path| {
                        Some(path.trim_start_matches('.').into())
                    })?;
                    break;
                }
            }
        }

        Ok(results)
    }

    fn files_from_archives(
        &self,
        filter: &[PackageRef],
        package_map: &PackageMap,
        interner: &Interner,
    ) -> Result<Vec<ArchiveResult>, PackageManagerError> {
        // Handle diversions: (parse output of dpkg-divert --list)
        tracing::debug!("Loading diversions");
        let diversions =
            divert::get_diversions(interner).wrap_err("Failed to get dpkg diversions")?;

        tracing::debug!("List of diversions: {diversions:?}");

        tracing::info!(
            "Loading file data from dpkg cache archives for {} packages",
            filter.len()
        );
        let archives = self.iterate_deb_archives(filter, package_map, interner)?;
        tracing::info!(
            "Got list of {} archives, starting extracting information (this may take a while, \
             especially on the first run before the disk cache can help)",
            filter.len()
        );
        let results: Vec<_> = archives
            .par_bridge()
            .map(|value| {
                value.and_then(|(pkg_ref, path)| {
                    Ok((
                        pkg_ref,
                        archive_to_entries(pkg_ref, &path, &diversions, package_map, interner)?,
                    ))
                })
            })
            .collect();
        tracing::info!("Extracted information from archives");
        Ok(results)
    }

    // Debian doesn't have enough info for konfigkoll in files(), use
    // files_from_archives() instead (and add a cache layer on top, since that
    // is slow)
    fn prefer_files_from_archive(&self) -> bool {
        true
    }
}

impl Debian {
    /// Find all deb archives for the given packages
    fn iterate_deb_archives<'inputs>(
        &'inputs self,
        filter: &'inputs [PackageRef],
        packages: &'inputs PackageMap,
        interner: &'inputs Interner,
    ) -> eyre::Result<
        impl Iterator<Item = Result<(PackageRef, PathBuf), ArchiveQueryError>> + 'inputs,
    > {
        let intermediate: Vec<_> = filter
            .iter()
            .map(|pkg_ref| {
                let pkg = packages
                    .get(pkg_ref)
                    .expect("Failed to find package in package map");
                // For deb ids[0] always exist and may contain the architecture if it is not the
                // primary
                let name = pkg.ids[0].as_str(interner);
                // Get the full deb file name
                let deb_filename = format_deb_filename(interner, pkg);

                (pkg_ref, name, deb_filename)
            })
            .collect();

        // Attempt to download all missing packages:
        let missing = missing_packages(
            &[CACHE_PATH],
            intermediate.iter().map(|(_, name, deb)| PackageQuery {
                package_match: deb,
                package: name,
            }),
        )?;

        if !missing.is_empty() {
            let _guard = self.pkgmgr_mutex.lock();
            tracing::info!("Downloading missing packages (installed but not in local cache)");
            download_debs(&missing)?;
        }

        let package_paths = intermediate
            .into_iter()
            .map(|(pkg_ref, name, deb_filename)| {
                let package_path =
                    locate_package_file(&[CACHE_PATH], &deb_filename, name, |pkg| {
                        let _guard = self.pkgmgr_mutex.lock();
                        download_deb(pkg)
                    })?;
                // Error if we couldn't find the package
                let package_path =
                    package_path.ok_or_else(|| ArchiveQueryError::PackageMissing {
                        query: *pkg_ref,
                        alternates: packages[pkg_ref].ids.clone(),
                    })?;
                Ok((*pkg_ref, package_path))
            });

        Ok(package_paths)
    }
}

/// Convert deb archives to file entries
fn archive_to_entries(
    pkg_ref: PackageRef,
    deb_file: &Path,
    diversions: &divert::Diversions,
    packages: &PackageMap,
    interner: &Interner,
) -> eyre::Result<Vec<FileEntry>> {
    tracing::debug!("Processing {}", deb_file.display());
    // The package is a .deb, which is actually an ar archive
    let package_file = File::open(deb_file)?;
    let mut archive = ar::Archive::new(package_file);

    // We want the data.tar.xz file (or other compression scheme)
    while let Some(entry) = archive.next_entry() {
        let mut entry = entry?;
        if entry.header().identifier().starts_with(b"data.tar") {
            let extension: CompactString = std::str::from_utf8(entry.header().identifier())?
                .split('.')
                .last()
                .ok_or_else(|| eyre::eyre!("No file extension found"))?
                .into();
            let mut decompressed = CompressionFormat::from_extension(&extension, &mut entry)?;
            let archive = tar::Archive::new(&mut decompressed);
            // Now, lets extract the requested files from the package
            let mut entries =
                convert_archive_entries(archive, pkg_ref, NAME, convert_deb_archive_path)?;

            let self_pkg = packages
                .get(&pkg_ref)
                .expect("Failed to find package in package map");
            for entry in &mut entries {
                // Apply diversions
                if let Some(diversion) = diversions.get(&entry.path) {
                    if !self_pkg.ids.contains(&diversion.by_package) {
                        // This file is diverted
                        tracing::debug!(
                            "Diverted file: {opath} -> {npath} by {diverting_pkg} while \
                             processing {pkg}",
                            opath = entry.path.display(),
                            npath = diversion.new_path.display(),
                            diverting_pkg = diversion.by_package.as_str(interner),
                            pkg = pkg_ref.as_str(interner),
                        );
                        entry.path.clone_from(&diversion.new_path);
                    }
                }
            }
            return Ok(entries);
        }
    }
    Err(eyre::eyre!("Failed to find data.tar in {deb_file:?}"))
}

/// Convert Debian archive paths to normal paths
fn convert_deb_archive_path(path: &Path) -> Option<Cow<'_, Path>> {
    // Remove leading .
    let p = path
        .as_os_str()
        .as_encoded_bytes()
        .trim_start_with(|ch| ch == '.');
    // If this is the root path, do not process it further (to prevent empty path)
    if p == b"/" {
        return Some(Cow::Borrowed(p.to_path().expect("Invalid path")));
    }
    // Otherwise strip any trailing slashes
    let p = p.trim_end_with(|ch| ch == '/');
    // Normally we don't need to add a leading / but some third party packages are
    // broken in this respect
    if p.starts_with(b"/") {
        return Some(Cow::Borrowed(p.to_path().expect("Invalid path")));
    }
    let p = bstr::concat([b"/", p]);
    return Some(Cow::Owned(p.into_path_buf().expect("Invalid path")));
}

/// Given a package name, try to figure out the full deb file name
fn format_deb_filename(interner: &Interner, package: &PackageInterned) -> String {
    format!(
        "{}_{}_{}.deb",
        package.name.as_str(interner),
        package.version.replace(':', "%3a"),
        package.architecture.map_or("*", |e| e.as_str(interner))
    )
}

/// Given a package name, try to figure out the full deb file name
fn guess_deb_file_name(interner: &Interner, pkg: &str, packages: &PackageMap) -> String {
    if let Some(pkgref) = interner.get(pkg) {
        // Yay, it is probably installed, we know what to look for
        if let Some(package) = packages.get(&PackageRef::new(pkgref)) {
            format_deb_filename(interner, package)
        } else {
            format!("{pkg}_*_*.deb")
        }
    } else {
        format!("{pkg}_*_*.deb")
    }
}

fn is_file_match(
    list_path: &Path,
    interner: &Interner,
    re: &RegexSet,
    paths: &[String],
    output: &OwningPackagesResult,
) -> eyre::Result<()> {
    let contents = std::fs::read_to_string(list_path)
        .wrap_err_with(|| format!("Failed to read {list_path:?}"))?;
    let matches = re.matches(&contents);
    if matches.matched_any() {
        let file_name = list_path
            .file_name()
            .ok_or_else(|| eyre::eyre!("Failed to extract filename"))?;
        let file_name = file_name.to_string_lossy();
        let file_name = file_name
            .strip_suffix(".list")
            .ok_or_else(|| eyre::eyre!("Not a list file?"))?;
        let pkg_name = match file_name.split_once(':') {
            Some((name, _arch)) => name,
            None => file_name,
        };
        let pkg: PackageRef = PackageRef::get_or_intern(interner, pkg_name);

        for m in matches {
            output.insert(paths[m].as_str().trim().into(), Some(pkg));
        }
    }
    Ok(())
}

fn merge_deb_fileentries(
    acc: &DashMap<PathBuf, FileEntry, ahash::RandomState>,
    files: Vec<FileEntry>,
    diversions: &divert::Diversions,
) {
    for mut file in files {
        // Apply diversions
        if let Some(diversion) = diversions.get(&file.path) {
            if Some(diversion.by_package) != file.package {
                // This file is diverted
                file.path.clone_from(&diversion.new_path);
            }
        }
        // Drop mutability
        let file = file;
        match acc.entry(file.path.clone()) {
            dashmap::mapref::entry::Entry::Vacant(vacant_entry) => {
                vacant_entry.insert(file);
            }
            dashmap::mapref::entry::Entry::Occupied(mut occupied_entry) => {
                let inner = occupied_entry.get_mut();
                // Checksum overwrites if it exists
                match file.properties {
                    Properties::RegularFileBasic(properties) => {
                        inner.properties = Properties::RegularFileBasic(properties);
                    }
                    Properties::Unknown => (),
                    _ => panic!("Impossible file type in deb parser"),
                }
            }
        }
    }
}

fn get_package_files(interner: &Interner) -> eyre::Result<impl Iterator<Item = Vec<FileEntry>>> {
    let files: Vec<_> = std::fs::read_dir(DB_PATH)?.collect();
    let results: eyre::Result<Vec<_>> = files
        .into_par_iter()
        .filter_map(|entry| match entry {
            Ok(entry) => {
                let results = process_file(interner, &entry);
                results.transpose()
            }
            Err(err) => Some(Err(err).wrap_err("Failed to get packages")),
        })
        .collect();
    Ok(results?.into_iter())
}

fn process_file(interner: &Interner, entry: &DirEntry) -> eyre::Result<Option<Vec<FileEntry>>> {
    let file_name = <Vec<u8> as ByteVec>::from_os_string(entry.file_name())
        .expect("Package names really should be valid Unicode on your platform");

    let result = match file_name.rsplit_once_str(b".") {
        Some((package_name, extension)) => {
            let package_ref = PackageRef::get_or_intern(interner, package_name.to_str_lossy());

            match extension {
                b"list" => {
                    let mut file = BufReader::new(File::open(entry.path())?);
                    Some(parsers::parse_paths(package_ref, &mut file)?)
                }
                b"md5sums" => {
                    let mut file = BufReader::new(File::open(entry.path())?);
                    Some(parsers::parse_md5sums(package_ref, &mut file)?)
                }
                _ => {
                    // Don't care
                    None
                }
            }
        }
        None => {
            // There are other files that we don't care about
            None
        }
    };
    Ok(result)
}

impl Packages for Debian {
    fn packages(&self, interner: &Interner) -> eyre::Result<Vec<PackageInterned>> {
        // Parse status
        tracing::debug!("Loading status to installed packages");
        let (_, mut packages) = {
            let mut status = BufReader::new(File::open(STATUS_PATH)?);
            parsers::parse_status(interner, &mut status, self.primary_architecture)
        }
        .context(format!("Failed to parse {STATUS_PATH}"))?;

        // Parse extended status
        tracing::debug!("Loading extended status to get auto installed packages");
        let extended_packages = {
            let mut status = BufReader::new(File::open(EXTENDED_STATUS_PATH)?);
            parsers::parse_extended_status(interner, &mut status)?
        };

        // We now need to update with auto installed status
        for package in packages.as_mut_slice() {
            let pkg_id = (
                package.name,
                package
                    .architecture
                    .ok_or_else(|| eyre::eyre!("No architecture"))?,
            );
            if let Some(Some(auto_installed)) = extended_packages.get(&pkg_id) {
                package.reason = Some(*auto_installed);
            }
        }

        Ok(packages)
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
                "apt-get",
                &["install", "--no-install-recommends"],
                install,
                (!ask_confirmation).then_some("-y"),
            )
            .wrap_err("Failed to install with apt-get")?;
        }
        if !uninstall.is_empty() {
            package_manager_transaction(
                "apt-get",
                &["remove"],
                uninstall,
                (!ask_confirmation).then_some("-y"),
            )
            .wrap_err("Failed to uninstall with apt-get")?;
        }
        Ok(())
    }

    fn mark(&self, dependencies: &[&str], manual: &[&str]) -> Result<(), PackageManagerError> {
        let _guard = self.pkgmgr_mutex.lock();
        if !dependencies.is_empty() {
            package_manager_transaction("apt-mark", &["auto"], dependencies, None)
                .wrap_err("Failed to mark auto-installed with apt-mark")?;
        }
        if !manual.is_empty() {
            package_manager_transaction("apt-mark", &["manual"], manual, None)
                .wrap_err("Failed to mark manual with apt-mark")?;
        }
        Ok(())
    }

    fn remove_unused(&self, ask_confirmation: bool) -> Result<(), PackageManagerError> {
        let _guard = self.pkgmgr_mutex.lock();
        package_manager_transaction(
            "apt-get",
            &["autoremove", "-o", "APT::Autoremove::SuggestsImportant=0"],
            &[],
            (!ask_confirmation).then_some("-y"),
        )
        .wrap_err("Failed to autoremove with apt-get")?;
        Ok(())
    }
}

// To get the original package file into the cache: apt install --reinstall -d
// pkgname /var/cache/apt/archives/pkgname_version_arch.deb
// arch: all, amd64, arm64, ...
// Epoch separator (normally :) is now %3a (URL encoded)

impl FullBackend for Debian {}

fn download_debs(pkgs: &[&str]) -> eyre::Result<()> {
    let status = std::process::Command::new("apt-get")
        .args([
            "install",
            "--reinstall",
            "-y",
            "--no-install-recommends",
            "-d",
        ])
        .args(pkgs)
        .status()?;
    if !status.success() {
        tracing::warn!("Failed to download package for {pkgs:?}");
    };
    Ok(())
}

fn download_deb(pkg: &str) -> eyre::Result<()> {
    let status = std::process::Command::new("apt-get")
        .args([
            "install",
            "--reinstall",
            "-y",
            "--no-install-recommends",
            "-d",
            pkg,
        ])
        .status()?;
    if !status.success() {
        tracing::warn!("Failed to download package for {pkg}");
    };
    Ok(())
}
