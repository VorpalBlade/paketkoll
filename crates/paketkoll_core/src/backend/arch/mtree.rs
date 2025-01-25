//! Logic to take mtree data to `FileEntry`

use dashmap::DashSet;
use eyre::OptionExt;
use eyre::WrapErr;
use flate2::bufread::GzDecoder;
use mtree2::MTree;
use paketkoll_types::files::Checksum;
use paketkoll_types::files::Directory;
use paketkoll_types::files::FileEntry;
use paketkoll_types::files::FileFlags;
use paketkoll_types::files::Gid;
use paketkoll_types::files::Mode;
use paketkoll_types::files::Properties;
use paketkoll_types::files::RegularFile;
use paketkoll_types::files::Symlink;
use paketkoll_types::files::Uid;
use paketkoll_types::intern::PackageRef;
use std::collections::BTreeSet;
use std::ffi::OsStr;
use std::fs::File;
use std::io::BufReader;
use std::io::Read;
use std::path::Path;
use std::path::PathBuf;

/// Set of special files to ignore from mtree data
///
/// These don't exist in the file system but do in the binary packages
/// themselves.
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
) -> eyre::Result<impl Iterator<Item = eyre::Result<FileEntry>> + use<'seen>> {
    let file = BufReader::new(File::open(path)?);
    let decoder = GzDecoder::new(file);
    if decoder.header().is_none() {
        eyre::bail!(
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
) -> eyre::Result<impl Iterator<Item = eyre::Result<FileEntry>> + 'input_data> {
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
        Err(err) => Some(Err(err).wrap_err("Error while parsing package")),
    });
    Ok(results)
}

/// Convert a single entry from mtree to a [`FileEntry`]
fn convert_mtree(
    pkg: PackageRef,
    item: &mtree2::Entry,
    seen_directories: &DashSet<(PathBuf, Directory)>,
    backup_files: &BTreeSet<Vec<u8>>,
) -> Result<Option<FileEntry>, eyre::Error> {
    Ok(match item.file_type() {
        Some(mtree2::FileType::Directory) => {
            let dir = Directory {
                owner: Uid::new(item.uid().ok_or_eyre("No uid for dir")?),
                group: Gid::new(item.gid().ok_or_eyre("No gid for dir")?),
                mode: Mode::new(item.mode().ok_or_eyre("Missing mode")?.into()),
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
                owner: Uid::new(item.uid().ok_or_eyre("No uid for file")?),
                group: Gid::new(item.gid().ok_or_eyre("No gid for file")?),
                mode: Mode::new(item.mode().ok_or_eyre("Missing mode")?.into()),
                mtime: item.time().ok_or_eyre("Missing mtime")?,
                checksum: Checksum::Sha256(*item.sha256().ok_or_eyre("Missing sha256")?),
                size: item.size().ok_or_eyre("Missing size")?,
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
                owner: Uid::new(item.uid().ok_or_eyre("No uid for link")?),
                group: Gid::new(item.gid().ok_or_eyre("No gid for link")?),
                target: item.link().ok_or_eyre("No target for link")?.into(),
            }),
            flags: FileFlags::empty(),
            source: super::NAME,
            seen: Default::default(),
        }),
        Some(
            mtree2::FileType::BlockDevice
            | mtree2::FileType::CharacterDevice
            | mtree2::FileType::Fifo
            | mtree2::FileType::Socket,
        )
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

/// Extract the path from an mtree entry, they start with a . which we want to
/// remove
fn extract_path(item: &mtree2::Entry) -> PathBuf {
    let path = item.path();
    let as_bytes = path.as_os_str().as_encoded_bytes();
    if as_bytes[0] == b'.' {
        // SAFETY:
        // * The encoding is "an unspecified, platform-specific, self-synchronizing
        //   superset of UTF-8"
        // * We are removing a leading ASCII character here (.).
        // * Thus, the buffer still contains the same superset of UTF-8
        PathBuf::from(unsafe { OsStr::from_encoded_bytes_unchecked(&as_bytes[1..]) })
    } else {
        path.into()
    }
}
