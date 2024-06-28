//! Logic to take mtree data to `FileEntry`

use std::{
    collections::BTreeSet,
    ffi::OsStr,
    fs::File,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};

use crate::types::{Directory, FileEntry, FileFlags, PackageRef, Properties, RegularFile, Symlink};
use anyhow::Context;
use dashmap::DashSet;
use flate2::read::GzDecoder;
use mtree2::{self, MTree};
use paketkoll_types::files::{Checksum, Gid, Mode, Uid};

/// Set of special files to ignore from mtree data
///
/// These don't exist in the file system but do in the binary packages themselves.
const SPECIAL_FILES: phf::Set<&'static [u8]> = phf::phf_set! {
    b"./.BUILDINFO",
    b"./.CHANGELOG",
    b"./.PKGINFO",
    b"./.INSTALL",
};

/// Extract data from compressed mtree file
pub(super) fn extract_mtree<'seen>(
    pkg: PackageRef,
    path: &Path,
    backup_files: BTreeSet<Vec<u8>>,
    seen_directories: &'seen DashSet<(PathBuf, Directory)>,
) -> anyhow::Result<impl Iterator<Item = anyhow::Result<FileEntry>> + 'seen> {
    let file = BufReader::new(File::open(path)?);
    let decoder = GzDecoder::new(file);
    if decoder.header().is_none() {
        anyhow::bail!(
            "Failed to open {:?} as gzip compressed (did Arch Linux change formats?)",
            path
        );
    }
    parse_mtree(pkg, decoder, backup_files, seen_directories)
}

/// Parse an mtree file from a [`std::io::Read`]
fn parse_mtree<'input_data>(
    pkg: PackageRef,
    reader: impl Read + 'input_data,
    backup_files: BTreeSet<Vec<u8>>,
    seen_directories: &'input_data DashSet<(PathBuf, Directory)>,
) -> anyhow::Result<impl Iterator<Item = anyhow::Result<FileEntry>> + 'input_data> {
    let mtree = MTree::from_reader(reader);
    let results = mtree.into_iter().filter_map(move |item| match item {
        Ok(inner) => {
            let raw = inner.path().as_os_str().as_encoded_bytes();
            // SPECIAL_FILES: These are files like .PKGINFO etc. Skip these.
            if SPECIAL_FILES.contains(raw) {
                None
            } else {
                convert_mtree(pkg, &inner, seen_directories, &backup_files).transpose()
            }
        }
        Err(err) => Some(Err(err).context("Error while parsing package")),
    });
    Ok(results)
}

/// Convert a single entry from mtree to a [`FileEntry`]
fn convert_mtree(
    pkg: PackageRef,
    item: &mtree2::Entry,
    seen_directories: &DashSet<(PathBuf, Directory)>,
    backup_files: &BTreeSet<Vec<u8>>,
) -> Result<Option<FileEntry>, anyhow::Error> {
    Ok(match item.file_type() {
        Some(mtree2::FileType::Directory) => {
            let dir = Directory {
                owner: Uid::new(item.uid().context("No uid for dir")?),
                group: Gid::new(item.gid().context("No gid for dir")?),
                mode: Mode(item.mode().context("Missing mode")?.into()),
            };
            let path = extract_path(item);
            if seen_directories.insert((path.clone(), dir.clone())) {
                Some(FileEntry {
                    package: Some(pkg),
                    path,
                    properties: Properties::Directory(dir),
                    flags: FileFlags::empty(),
                    source: super::NAME,
                    seen: Default::default(),
                })
            } else {
                None
            }
        }
        Some(mtree2::FileType::File) => Some(FileEntry {
            package: Some(pkg),
            path: extract_path(item),
            properties: Properties::RegularFile(RegularFile {
                owner: Uid::new(item.uid().context("No uid for file")?),
                group: Gid::new(item.gid().context("No gid for file")?),
                mode: Mode(item.mode().context("Missing mode")?.into()),
                mtime: item.time().context("Missing mtime")?,
                checksum: Checksum::Sha256(*item.sha256().context("Missing sha256")?),
                size: item.size().context("Missing size")?,
            }),
            flags: if backup_files.contains(item.path().as_os_str().as_encoded_bytes()) {
                FileFlags::CONFIG
            } else {
                FileFlags::empty()
            },
            source: super::NAME,
            seen: Default::default(),
        }),
        Some(mtree2::FileType::SymbolicLink) => Some(FileEntry {
            package: Some(pkg),
            path: extract_path(item),
            properties: Properties::Symlink(Symlink {
                owner: Uid::new(item.uid().context("No uid for link")?),
                group: Gid::new(item.gid().context("No gid for link")?),
                target: item.link().context("No target for link")?.into(),
            }),
            flags: FileFlags::empty(),
            source: super::NAME,
            seen: Default::default(),
        }),
        Some(mtree2::FileType::BlockDevice)
        | Some(mtree2::FileType::CharacterDevice)
        | Some(mtree2::FileType::Fifo)
        | Some(mtree2::FileType::Socket)
        | None => Some(FileEntry {
            package: Some(pkg),
            path: extract_path(item),
            properties: Properties::Special {},
            flags: FileFlags::empty(),
            source: super::NAME,
            seen: Default::default(),
        }),
    })
}

/// Extract the path from an mtree entry, they start with a . which we want to remove
fn extract_path(item: &mtree2::Entry) -> PathBuf {
    let path = item.path();
    let as_bytes = path.as_os_str().as_encoded_bytes();
    if as_bytes[0] == b'.' {
        // SAFETY:
        // * The encoding is "an unspecified, platform-specific, self-synchronizing superset of UTF-8"
        // * We are removing a leading ASCII character here (.).
        // * Thus the buffer still contains the same superset of UTF-8
        PathBuf::from(unsafe { OsStr::from_encoded_bytes_unchecked(&as_bytes[1..]) })
    } else {
        path.into()
    }
}
