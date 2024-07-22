//! Various utility functions

use ahash::{AHashMap, AHashSet};
use anyhow::Context;
use compact_str::CompactString;
use smallvec::SmallVec;
use std::{
    io::{BufReader, Read},
    os::unix::process::ExitStatusExt,
    path::PathBuf,
};

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

#[cfg(feature = "__extraction")]
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

/// Attempt to search a directory based cache and if not found, download the package
#[cfg(feature = "__extraction")]
pub(crate) fn locate_package_file(
    dir_candidates: &[&str],
    package_match: &str,
    pkg: &str,
    download_pkg: impl Fn(&str) -> Result<(), anyhow::Error>,
) -> Result<Option<PathBuf>, anyhow::Error> {
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
                    let mut paths: SmallVec<[_; 5]> = paths.collect::<Result<_, _>>()?;
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

/// Extract files from a generic tar archive
#[cfg(feature = "__extraction")]
pub(crate) fn extract_files(
    mut archive: tar::Archive<impl Read>,
    queries: &AHashSet<&str>,
    results: &mut AHashMap<paketkoll_types::backend::OriginalFileQuery, Vec<u8>>,
    pkg: &str,
    name_manger: impl Fn(&str) -> CompactString,
) -> Result<(), anyhow::Error> {
    let mut seen = AHashSet::new();

    for entry in archive
        .entries()
        .context("Failed to read package archive")?
    {
        let mut entry = entry?;
        let path = entry.path()?;
        let path = path
            .to_str()
            .ok_or_else(|| anyhow::anyhow!("Failed to convert path to string"))?;
        let path = name_manger(path);
        if let Some(pkg_idx) = queries.get(path.as_str()) {
            seen.insert(*pkg_idx);
            let mut contents = Vec::new();
            entry.read_to_end(&mut contents)?;
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
        log::error!("Failed to find requested file {missing} in package {pkg}");
        has_errors = true;
    }
    if has_errors {
        anyhow::bail!("Failed to find requested files in package {pkg}");
    };
    Ok(())
}
