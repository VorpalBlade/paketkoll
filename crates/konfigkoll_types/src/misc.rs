use std::borrow::Cow;
use std::fs::File;
use std::hash::Hash;
use std::hash::Hasher;
use std::io::BufReader;
use std::io::Read;

use anyhow::Context;
use camino::Utf8Path;
use either::Either;

use paketkoll_types::files::Checksum;

/// Describes the contents of a file for the purpose of a [`FsOp`](crate::FsOp).
#[derive(Debug, Clone)]
pub enum FileContents {
    /// Literal data
    Literal { checksum: Checksum, data: Box<[u8]> },
    /// From a file, for use when the data is too big to fit comfortably in
    /// memory
    FromFile {
        checksum: Checksum,
        path: camino::Utf8PathBuf,
    },
}

impl FileContents {
    pub fn from_literal(data: Box<[u8]>) -> Self {
        let checksum = paketkoll_utils::checksum::sha256_buffer(&data);
        Self::Literal { checksum, data }
    }

    pub fn from_file(path: &Utf8Path) -> anyhow::Result<Self> {
        let mut reader =
            BufReader::new(File::open(path).with_context(|| format!("Failed to open {path}"))?);
        let checksum =
            paketkoll_utils::checksum::sha256_readable(&mut reader).context("Checksum failed")?;
        Ok(Self::FromFile {
            checksum,
            path: path.to_owned(),
        })
    }

    pub fn checksum(&self) -> &Checksum {
        match self {
            FileContents::Literal { checksum, .. } => checksum,
            FileContents::FromFile { checksum, .. } => checksum,
        }
    }

    /// Get a readable for the data in this operation
    pub fn readable(&self) -> anyhow::Result<impl Read + '_> {
        match self {
            FileContents::Literal { checksum: _, data } => Ok(Either::Left(data.as_ref())),
            FileContents::FromFile { checksum: _, path } => Ok(Either::Right(
                File::open(path).with_context(|| format!("Failed to open {path}"))?,
            )),
        }
    }

    pub fn contents(&self) -> anyhow::Result<Cow<'_, [u8]>> {
        match self {
            FileContents::Literal { data, .. } => Ok(Cow::Borrowed(data.as_ref())),
            FileContents::FromFile { path, .. } => {
                let mut reader = BufReader::new(
                    File::open(path).with_context(|| format!("Failed to open {path}"))?,
                );
                let mut data = Vec::new();
                reader.read_to_end(&mut data)?;
                Ok(Cow::Owned(data))
            }
        }
    }
}

impl PartialEq for FileContents {
    fn eq(&self, other: &Self) -> bool {
        self.checksum() == other.checksum()
    }
}

impl Eq for FileContents {}

impl PartialOrd for FileContents {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for FileContents {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.checksum().cmp(other.checksum())
    }
}

impl Hash for FileContents {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.checksum().hash(state);
    }
}
