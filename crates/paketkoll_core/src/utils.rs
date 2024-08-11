//! Various utility functions

use std::io::BufReader;
use std::io::Read;
use std::os::unix::process::ExitStatusExt;
use std::path::PathBuf;

use ahash::AHashMap;
use ahash::AHashSet;
use anyhow::Context;
use compact_str::CompactString;
use smallvec::SmallVec;

/// Helper to do a generic package manager transaction
pub(crate) fn package_manager_transaction(
    program_name: &str,
    flags: &[&str],
    pkg_list: &[&str],
    ask_confirmation: Option<&str>,
) -> anyhow::Result<()> {
    let mut cmd = std::process::Command::new(program_name);
    for arg in flags {
        cmd.arg(arg);
    }
    if let Some(flag) = ask_confirmation {
        cmd.arg(flag);
    }
    for pkg in pkg_list {
        cmd.arg(pkg);
    }
    let status = cmd
        .status()
        .with_context(|| format!("Failed to execute {program_name}"))?;
    if !status.success() {
        match status.code() {
            Some(code) => anyhow::bail!("{program_name} failed with exit code {code}"),
            _ => anyhow::bail!("{program_name} failed with signal {:?}", status.signal()),
        }
    }
    Ok(())
}

pub(crate) enum CompressionFormat<'archive, R: Read + 'archive> {
    #[cfg(feature = "__gzip")]
    Gzip(flate2::read::GzDecoder<R>),
    #[cfg(feature = "__xz")]
    Xz(xz2::read::XzDecoder<R>),
    #[cfg(feature = "__bzip2")]
    Bzip2(bzip2::read::BzDecoder<R>),
    #[cfg(feature = "__zstd")]
    Zstd(zstd::stream::Decoder<'archive, BufReader<R>>),
}

impl<'archive, R: Read + 'archive> CompressionFormat<'archive, R> {
    pub(crate) fn from_extension(ext: &str, stream: R) -> anyhow::Result<Self> {
        match ext {
            #[cfg(feature = "__gzip")]
            "gz" => Ok(Self::Gzip(flate2::read::GzDecoder::new(stream))),
            #[cfg(feature = "__xz")]
            "xz" => Ok(Self::Xz(xz2::read::XzDecoder::new(stream))),
            #[cfg(feature = "__bzip2")]
            "bz2" => Ok(Self::Bzip2(bzip2::read::BzDecoder::new(stream))),
            #[cfg(feature = "__zstd")]
            "zst" | "zstd" => Ok(Self::Zstd(zstd::stream::Decoder::new(stream)?)),
            _ => Err(anyhow::anyhow!("Unknown compression format: {ext}")),
        }
    }
}

impl<'archive, R: Read + 'archive> Read for CompressionFormat<'archive, R> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        match self {
            #[cfg(feature = "__gzip")]
            Self::Gzip(inner) => inner.read(buf),
            #[cfg(feature = "__xz")]
            Self::Xz(inner) => inner.read(buf),
            #[cfg(feature = "__bzip2")]
            Self::Bzip2(inner) => inner.read(buf),
            #[cfg(feature = "__zstd")]
            Self::Zstd(inner) => inner.read(buf),
        }
    }
}

pub(crate) fn group_queries_by_pkg(
    queries: &[paketkoll_types::backend::OriginalFileQuery],
) -> AHashMap<&str, AHashSet<&str>> {
    let mut queries_by_pkg: AHashMap<&str, AHashSet<&str>> = AHashMap::new();

    for query in queries {
        queries_by_pkg
            .entry(query.package.as_str())
            .and_modify(|v| {
                v.insert(query.path.as_str());
            })
            .or_insert_with(|| {
                let mut set = AHashSet::new();
                set.insert(query.path.as_str());
                set
            });
    }
    queries_by_pkg
}

/// Attempt to search a directory based cache and if not found, download the
/// package
pub(crate) fn locate_package_file(
    dir_candidates: &[&str],
    package_match: &str,
    pkg: &str,
    download_pkg: impl Fn(&str) -> Result<(), anyhow::Error>,
) -> Result<Option<PathBuf>, paketkoll_types::backend::OriginalFileError> {
    for downloaded in [false, true] {
        // Try to locate package
        for dir in dir_candidates.iter() {
            let path = format!("{}/{}", dir, package_match);
            let entries = glob::glob_with(
                &path,
                glob::MatchOptions {
                    case_sensitive: true,
                    require_literal_separator: true,
                    require_literal_leading_dot: true,
                },
            );
            match entries {
                Ok(paths) => {
                    let mut paths: SmallVec<[_; 5]> =
                        paths.collect::<Result<_, _>>().context("Glob error")?;
                    paths.sort();
                    if paths.len() > 1 {
                        log::warn!(
                            "Found multiple matches for {pkg}, taking latest in sort order: {}",
                            paths
                                .last()
                                .expect("We know there is at least one")
                                .display()
                        );
                    }
                    if !paths.is_empty() {
                        return Ok(paths.last().cloned());
                    }
                }
                Err(_) => continue,
            }
        }

        // Nothing found, try downloading the package
        if downloaded {
            log::error!("Failed to find package for {pkg}");
        } else {
            log::info!("Downloading package for {pkg}");
            download_pkg(pkg)?;
        }
    }
    Ok(None)
}

pub(crate) struct PackageQuery<'a> {
    pub(crate) package_match: &'a str,
    pub(crate) package: &'a str,
}

/// Attempt to search a directory based cache and return which packages are
/// missing
pub(crate) fn missing_packages<'strings>(
    dir_candidates: &[&str],
    package_matches: impl Iterator<Item = PackageQuery<'strings>>,
) -> Result<Vec<&'strings str>, anyhow::Error> {
    let mut missing = vec![];
    // Try to locate package
    for PackageQuery {
        package_match,
        package,
    } in package_matches
    {
        for dir in dir_candidates.iter() {
            let path = format!("{}/{}", dir, package_match);
            let entries = glob::glob_with(
                &path,
                glob::MatchOptions {
                    case_sensitive: true,
                    require_literal_separator: true,
                    require_literal_leading_dot: true,
                },
            );
            match entries {
                Ok(paths) => {
                    let mut paths: SmallVec<[_; 5]> = paths.collect::<Result<_, _>>()?;
                    paths.sort();
                    if paths.len() > 1 {
                        log::warn!(
                            "Found multiple matches for {package}, taking latest in sort order: {}",
                            paths
                                .last()
                                .expect("We know there is at least one")
                                .display()
                        );
                    }
                    if paths.is_empty() {
                        missing.push(package);
                    }
                }
                Err(_) => continue,
            }
        }
    }
    Ok(missing)
}

/// Extract files from a generic tar archive
pub(crate) fn extract_files(
    mut archive: tar::Archive<impl Read>,
    queries: &AHashSet<&str>,
    results: &mut paketkoll_types::backend::OriginalFilesResult,
    pkg: &str,
    name_map_filter: impl Fn(&str) -> Option<CompactString>,
) -> Result<(), paketkoll_types::backend::OriginalFileError> {
    use paketkoll_types::backend::OriginalFileError;

    let mut seen = AHashSet::new();

    for entry in archive
        .entries()
        .context("Failed to read package archive")?
    {
        let mut entry = entry.context("TAR parsing error (entry)")?;
        let path = entry.path().context("TAR parsing error (path)")?;
        let path = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Failed to convert path to string"))?;
        let path = match name_map_filter(path) {
            Some(v) => v,
            None => continue,
        };
        if let Some(pkg_idx) = queries.get(path.as_str()) {
            seen.insert(*pkg_idx);
            let mut contents = Vec::new();
            entry
                .read_to_end(&mut contents)
                .context("TAR parsing error (file contents)")?;
            results.insert(
                paketkoll_types::backend::OriginalFileQuery {
                    package: pkg.into(),
                    path,
                },
                contents,
            );
            // Check if we can exit early from processing this package
            if seen.len() == queries.len() {
                break;
            }
        }
    }
    let diff = queries.difference(&*seen);
    let mut has_errors = false;
    for missing in diff {
        log::warn!("Failed to find requested file {missing} in package {pkg}");
        has_errors = true;
    }
    if has_errors {
        return Err(OriginalFileError::FileNotFound(pkg.into()));
    };
    Ok(())
}

/// Convert a stream of tar entries to a list of file entries
pub(crate) fn convert_archive_entries(
    mut archive: tar::Archive<impl std::io::Read>,
    pkg_ref: paketkoll_types::intern::PackageRef,
    source: &'static str,
    name_map_filter: impl Fn(&std::path::Path) -> Option<std::borrow::Cow<'_, std::path::Path>>,
) -> Result<Vec<paketkoll_types::files::FileEntry>, anyhow::Error> {
    use std::time::SystemTime;

    use paketkoll_types::files::Directory;
    use paketkoll_types::files::FileEntry;
    use paketkoll_types::files::FileFlags;
    use paketkoll_types::files::Gid;
    use paketkoll_types::files::Mode;
    use paketkoll_types::files::Properties;
    use paketkoll_types::files::RegularFile;
    use paketkoll_types::files::Symlink;
    use paketkoll_types::files::Uid;
    use paketkoll_utils::checksum::sha256_readable;

    let mut results = AHashMap::new();
    for entry in archive
        .entries()
        .context("Failed to read package archive")?
    {
        let mut entry = entry?;
        let path = entry.path()?;
        let path = path.as_ref();
        let path = match name_map_filter(path) {
            Some(v) => v.into_owned(),
            None => continue,
        };
        let mode = Mode::new(entry.header().mode()?);
        let owner = Uid::new(entry.header().uid()?.try_into()?);
        let group = Gid::new(entry.header().gid()?.try_into()?);
        match entry.header().entry_type() {
            tar::EntryType::Regular | tar::EntryType::Continuous => {
                let size = entry.size();
                assert_eq!(size, entry.header().size()?);
                let mtime = entry.header().mtime()?;
                results.insert(
                    path.clone(),
                    FileEntry {
                        package: Some(pkg_ref),
                        path,
                        properties: Properties::RegularFile(RegularFile {
                            mode,
                            owner,
                            group,
                            size,
                            mtime: SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(mtime),
                            checksum: sha256_readable(&mut entry)?,
                        }),
                        flags: FileFlags::empty(),
                        source,
                        seen: Default::default(),
                    },
                );
            }
            tar::EntryType::Link | tar::EntryType::GNULongLink => {
                let link = entry.link_name()?.expect("No link name");
                let link = name_map_filter(link.as_ref())
                    .expect("Filtered link name")
                    .into_owned();
                let existing = results
                    .get(&link)
                    .expect("Links must refer to already archived files");
                let mut new = existing.clone();
                new.path = path.clone();
                results.insert(path.clone(), new);
            }
            tar::EntryType::Symlink => {
                let link = entry.link_name()?;
                results.insert(
                    path.clone(),
                    FileEntry {
                        package: Some(pkg_ref),
                        path,
                        properties: Properties::Symlink(Symlink {
                            owner,
                            group,
                            target: link
                                .ok_or(anyhow::anyhow!("Failed to get link target"))?
                                .into(),
                        }),
                        flags: FileFlags::empty(),
                        source,
                        seen: Default::default(),
                    },
                );
            }
            tar::EntryType::Char | tar::EntryType::Block | tar::EntryType::Fifo => {
                results.insert(
                    path.clone(),
                    FileEntry {
                        package: Some(pkg_ref),
                        path,
                        properties: Properties::Special,
                        flags: FileFlags::empty(),
                        source,
                        seen: Default::default(),
                    },
                );
            }
            tar::EntryType::Directory => {
                results.insert(
                    path.clone(),
                    FileEntry {
                        package: Some(pkg_ref),
                        path,
                        properties: Properties::Directory(Directory { mode, owner, group }),
                        flags: FileFlags::empty(),
                        source,
                        seen: Default::default(),
                    },
                );
            }
            tar::EntryType::GNUSparse
            | tar::EntryType::GNULongName
            | tar::EntryType::XGlobalHeader
            | tar::EntryType::XHeader => todo!(),
            _ => todo!(),
        }
    }
    Ok(results.into_values().collect())
}
