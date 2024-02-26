//! Backend for Debian and derivatives
mod divert;
mod parsers;

use std::fs::{DirEntry, File};
use std::io::BufReader;
use std::path::PathBuf;

use anyhow::Context;
use bstr::ByteSlice;
use bstr::ByteVec;
use dashmap::DashMap;
use rayon::prelude::*;

use crate::types::{FileEntry, PackageInterner, PackageRef, Properties};

use super::{Files, Name};

// Each package has a set of files in DB_PATH:
// *.list (all installed paths, one per line, including directories)
// *.md5sums (md5sum<space>path, one per line for all regular files)
// *.conffiles (may not exist, one file name per line)
// There are other files we don't care about (.symbols, .postinst, ...)
//
// Special files: /var/lib/dpkg/info/format (contains "1")
//
// Config files have no checksums in md5sums, so we need to parse /var/lib/dpkg/status for that.

const DB_PATH: &str = "/var/lib/dpkg/info";
const STATUS_PATH: &str = "/var/lib/dpkg/status";

#[derive(Debug)]
pub(crate) struct Debian;

#[derive(Debug, Default)]
pub(crate) struct DebianBuilder {}

impl DebianBuilder {
    pub fn build(self) -> Debian {
        Debian
    }
}

impl Name for Debian {
    fn name(&self) -> &'static str {
        "Debian"
    }
}

impl Files for Debian {
    fn files(
        &self,
        interner: &crate::types::PackageInterner,
    ) -> anyhow::Result<Vec<crate::types::FileEntry>> {
        log::debug!(target: "paketkoll_core::backend::deb", "Loading packages");
        let packages_files: Vec<_> = get_package_files(interner)?.collect();

        // Handle diversions: (parse output of dpkg-divert --list)
        log::debug!(target: "paketkoll_core::backend::deb", "Loading diversions");
        let diversions =
            divert::get_diverions(interner).context("Failed to get dpkg diversions")?;

        // Load config files.
        log::debug!(target: "paketkoll_core::backend::deb", "Loading status to get config files");
        let config_files = {
            let mut status = BufReader::new(File::open(STATUS_PATH)?);
            parsers::parse_status(interner, &mut status)
        }
        .context(format!("Failed to parse {}", STATUS_PATH))?;

        log::debug!(target: "paketkoll_core::backend::deb", "Merging packages files into one map");
        let merged = DashMap::new();
        packages_files.into_par_iter().for_each(|files| {
            merge_deb_fileentries(&merged, files, &diversions);
        });

        // The config files must be merged into the results
        log::debug!(target: "paketkoll_core::backend::deb", "Merging config files");
        merge_deb_fileentries(&merged, config_files, &diversions);

        // Finally extract just the file entries
        Ok(merged.into_iter().map(|(_, v)| v).collect())
    }
}

fn merge_deb_fileentries(
    acc: &DashMap<PathBuf, FileEntry>,
    files: Vec<FileEntry>,
    diversions: &divert::Diversions,
) {
    for mut file in files {
        // Apply diversions
        if let Some(diversion) = diversions.get(&file.path) {
            if Some(diversion.by_package) != file.package {
                // This file is diverted
                file.path = diversion.new_path.clone();
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

fn get_package_files(
    interner: &PackageInterner,
) -> anyhow::Result<impl Iterator<Item = Vec<FileEntry>>> {
    let files: Vec<_> = std::fs::read_dir(DB_PATH)?.collect();
    let results: anyhow::Result<Vec<_>> = files
        .into_par_iter()
        .filter_map(|entry| match entry {
            Ok(entry) => {
                let results = process_file(interner, &entry);
                results.transpose()
            }
            Err(err) => Some(Err(err).context("Failed to get packages")),
        })
        .collect();
    Ok(results?.into_iter())
}

fn process_file(
    interner: &PackageInterner,
    entry: &DirEntry,
) -> anyhow::Result<Option<Vec<FileEntry>>> {
    let file_name = <Vec<u8> as ByteVec>::from_os_string(entry.file_name())
        .expect("Package names really should be valid Unicode on your platform");

    let result = match file_name.rsplit_once_str(b".") {
        Some((package_name, extension)) => {
            let package_ref = PackageRef(interner.get_or_intern(package_name.to_str_lossy()));

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
