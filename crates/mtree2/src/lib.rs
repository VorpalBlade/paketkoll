//! A library for iterating through entries of an mtree.
//!
//! *mtree* is a data format used for describing a sequence of files. Their
//! location is record, along with optional extra values like checksums, size,
//! permissions etc.
//!
//! For details on the spec see [mtree(5)](https://www.freebsd.org/cgi/man.cgi?mtree(5)).
//!
//! # Examples
//!
//! ```
//! use mtree2::MTree;
//! use std::time::{SystemTime, UNIX_EPOCH};
//!
//! // We're going to load data from a string so this example with pass doctest,
//! // but there's no reason you can't use a file, or any other data source.
//! let raw_data = "
//! /set type=file uid=0 gid=0 mode=644
//! ./.BUILDINFO time=1523250074.300237174 size=8602 md5digest=13c0a46c2fb9f18a1a237d4904b6916e \
//!     sha256digest=db1941d00645bfaab04dd3898ee8b8484874f4880bf03f717adf43a9f30d9b8c
//! ./.PKGINFO time=1523250074.276237110 size=682 md5digest=fdb9ac9040f2e78f3561f27e5b31c815 \
//!     sha256digest=5d41b48b74d490b7912bdcef6cf7344322c52024c0a06975b64c3ca0b4c452d1
//! /set mode=755
//! ./usr time=1523250049.905171912 type=dir
//! ./usr/bin time=1523250065.373213293 type=dir
//! ";
//! let entries = MTree::from_reader(raw_data.as_bytes());
//! for entry in entries {
//!     // Normally you'd want to handle any errors
//!     let entry = entry.unwrap();
//!     // We can print out a human-readable copy of the entry
//!     println!("{}", entry);
//!     // Let's check that if there is a modification time, it's in the past
//!     if let Some(time) = entry.time() {
//!         assert!(time < SystemTime::now());
//!     }
//!     // We might also want to take a checksum of the file, and compare it to the digests
//!     // supplied by mtree, but this example doesn't have access to a filesystem.
//! }
//! ```
//! # Crate features
//!
//! By default, pathnames are parsed as ASCII, with characters outside of the 95
//! printable ASCII characters encoded as backshlash followed by three octal digits according to [mtree(5)](https://www.freebsd.org/cgi/man.cgi?mtree(5)).
//!
//!
//! Parsing of [strsvis VIS_CSTYLE](https://man.netbsd.org/strsvis.3)
//! coded characters is added in addition as specified in [mtree(8)](https://man.netbsd.org/mtree.8) for the netbsd6 flavor.

pub use parser::FileMode;
pub use parser::FileType;
pub use parser::Format;
use parser::Keyword;
use parser::LineParseError;
use parser::MTreeLine;
pub use parser::ParserError;
use parser::SpecialKind;
use std::env;
use std::ffi::OsStr;
use std::fmt;
use std::io::BufRead;
use std::io::BufReader;
use std::io::Read;
use std::io::Split;
use std::io::{self};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::path::PathBuf;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;
use util::decode_escapes_path;

mod parser;
mod util;

#[cfg(not(unix))]
compiler_error!("This library currently only supports unix, due to windows using utf-16 for paths");

/// An mtree parser (start here).
///
/// This is the main struct for the lib. Semantically, an mtree file is a
/// sequence of filesystem records. These are provided as an iterator. Use the
/// `from_reader` function to construct an instance.
pub struct MTree<R>
where
    R: Read,
{
    /// The iterator over lines (lines are guaranteed to end in \n since we only
    /// support unix).
    inner: Split<BufReader<R>>,
    /// The current working directory for dir calculations.
    cwd: PathBuf,
    /// These are set with the '/set' and '/unset' special functions.
    default_params: Params,
}

impl<R> MTree<R>
where
    R: Read,
{
    /// The constructor function for an `MTree` instance.
    ///
    /// This uses the current working directory as the base for relative paths.
    /// Relative specifications are allowed to exceed the starting level but are
    /// truncated to the root.
    pub fn from_reader(reader: R) -> Self {
        Self {
            inner: BufReader::new(reader).split(b'\n'),
            cwd: env::current_dir().unwrap_or_default(),
            default_params: Params::default(),
        }
    }

    /// The constructor function for an `MTree` instance.
    ///
    /// This uses the provided path  as the base for relative paths.
    /// Relative specifications are allowed to exceed the starting level but are
    /// truncated to the root.
    pub fn from_reader_with_cwd(reader: R, cwd: PathBuf) -> Self {
        Self {
            inner: BufReader::new(reader).split(b'\n'),
            cwd,
            default_params: Params::default(),
        }
    }

    /// The constructor function for an `MTree` instance.
    ///
    /// This uses an empty `PathBuf` as the base for relative paths.
    /// Relative specifications are allowed to exceed the starting level but are
    /// truncated to the root.
    pub fn from_reader_with_empty_cwd(reader: R) -> Self {
        Self {
            inner: BufReader::new(reader).split(b'\n'),
            cwd: PathBuf::new(),
            default_params: Params::default(),
        }
    }

    /// This is a helper function to make error handling easier.
    fn next_entry(
        &mut self,
        line: Result<Vec<u8>, LineParseError>,
    ) -> Result<Option<Entry>, LineParseError> {
        let line = line?;
        let line = MTreeLine::from_bytes(&line)?;
        Ok(match line {
            MTreeLine::Blank | MTreeLine::Comment => None,
            MTreeLine::Special(SpecialKind::Set, keywords) => {
                self.default_params.set_list(keywords.into_iter());
                None
            }
            // this won't work because keywords need to be parsed without arguments.
            MTreeLine::Special(SpecialKind::Unset, _keywords) => unimplemented!(),
            MTreeLine::Relative(path, keywords) => {
                let mut params = self.default_params.clone();
                params.set_list(keywords.into_iter());
                let filepath = self.cwd.join(&path);
                if params.file_type == Some(FileType::Directory) {
                    self.cwd.push(&path);
                }

                Some(Entry {
                    path: filepath,
                    params,
                })
            }
            MTreeLine::DotDot => {
                // Only pop if we're not at root or first level
                if self.cwd.parent().and_then(|p| p.parent()).is_some() {
                    self.cwd.pop();
                }
                None
            }
            MTreeLine::Full(path, keywords) => {
                let mut params = self.default_params.clone();
                params.set_list(keywords.into_iter());
                Some(Entry { path, params })
            }
        })
    }
}

impl<R> Iterator for MTree<R>
where
    R: Read,
{
    type Item = Result<Entry, Error>;

    fn next(&mut self) -> Option<Result<Entry, Error>> {
        let mut accumulated_line: Option<Vec<u8>> = None;

        while let Some(line_result) = self.inner.next() {
            // Handle IO errors from reading the line
            let line = match line_result {
                Ok(line) => line,
                Err(e) => return Some(Err(Error::Io(e))),
            };

            // If we have an accumulated line, append to it, otherwise use current line
            let current_input = if let Some(mut acc) = accumulated_line.take() {
                acc.extend_from_slice(&line);
                Ok(acc)
            } else {
                Ok(line)
            };

            // Try to parse the current input into an entry
            match self.next_entry(current_input) {
                Ok(Some(entry)) => return Some(Ok(entry)),
                Ok(None) => continue, // Skip blank lines, comments etc
                Err(LineParseError::WrappedLine(partial)) => {
                    accumulated_line = Some(partial);
                    continue;
                }
                Err(LineParseError::Io(e)) => return Some(Err(Error::Io(e))),
                Err(LineParseError::Parser(e)) => return Some(Err(Error::Parser(e))),
            }
        }

        // At EOF - handle any remaining accumulated line
        if let Some(final_line) = accumulated_line {
            // Try to parse the final accumulated line
            match self.next_entry(Ok(final_line)) {
                Ok(Some(entry)) => Some(Ok(entry)),
                Ok(None) => None,
                Err(LineParseError::WrappedLine(_)) => {
                    // This shouldn't happen since we checked for trailing backslash
                    Some(Err(Error::Parser(ParserError(
                        "unexpected wrapped line at end of file".to_owned(),
                    ))))
                }
                Err(LineParseError::Io(e)) => Some(Err(Error::Io(e))),
                Err(LineParseError::Parser(e)) => Some(Err(Error::Parser(e))),
            }
        } else {
            None
        }
    }
}

/// An entry in the mtree file.
///
/// Entries have a path to the entity in question, and a list of optional
/// params.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Entry {
    /// The path of this entry
    path: PathBuf,
    /// All parameters applicable to this entry
    params: Params,
}

impl fmt::Display for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, r#"mtree entry for "{}""#, self.path.display())?;
        write!(f, "{}", self.params)
    }
}

impl Entry {
    /// The path of this entry
    pub fn path(&self) -> &Path {
        self.path.as_ref()
    }

    /// `cksum` The checksum of the file using the default algorithm specified
    /// by the cksum(1) utility.
    pub fn checksum(&self) -> Option<u64> {
        self.params.checksum
    }

    /// `device` The device number for *block* or *char* file types.
    pub fn device(&self) -> Option<&Device> {
        self.params.device.as_deref()
    }

    /// `contents` The full pathname of a file that holds the contents of this
    /// file.
    pub fn contents(&self) -> Option<&Path> {
        self.params.contents.as_ref().map(AsRef::as_ref)
    }

    /// `flags` The file flags as a symbolic name.
    pub fn flags(&self) -> Option<&[u8]> {
        self.params.flags.as_ref().map(AsRef::as_ref)
    }

    /// `gid` The file group as a numeric value.
    pub fn gid(&self) -> Option<u32> {
        self.params.gid
    }

    /// `gname` The file group as a symbolic name.
    ///
    /// The name can be up to 32 chars and must match regex
    /// `[a-z_][a-z0-9_-]*[$]?`.
    pub fn gname(&self) -> Option<&[u8]> {
        self.params.gname.as_ref().map(AsRef::as_ref)
    }

    /// `ignore` Ignore any file hierarchy below this line.
    pub fn ignore(&self) -> bool {
        self.params.ignore
    }

    /// `inode` The inode number.
    pub fn inode(&self) -> Option<u64> {
        self.params.inode
    }

    /// `link` The target of the symbolic link when type=link.
    pub fn link(&self) -> Option<&Path> {
        self.params.link.as_ref().map(AsRef::as_ref)
    }

    /// `md5|md5digest` The MD5 message digest of the file.
    pub fn md5(&self) -> Option<u128> {
        self.params.md5
    }

    /// `mode` The current file's permissions as a numeric (octal) or symbolic
    /// value.
    pub fn mode(&self) -> Option<FileMode> {
        self.params.mode
    }

    /// `nlink` The number of hard links the file is expected to have.
    pub fn nlink(&self) -> Option<u64> {
        self.params.nlink
    }

    /// `nochange` Make sure this file or directory exists but otherwise ignore
    /// all attributes.
    pub fn no_change(&self) -> bool {
        self.params.no_change
    }

    /// `optional` The file is optional; do not complain about the file if it is
    /// not in the file hierarchy.
    pub fn optional(&self) -> bool {
        self.params.optional
    }

    /// `resdevice` The "resident" device number of the file, e.g. the ID of the
    /// device that contains the file. Its format is the same as the one for
    /// `device`.
    pub fn resident_device(&self) -> Option<&Device> {
        self.params.resident_device.as_deref()
    }

    /// `rmd160|rmd160digest|ripemd160digest` The RIPEMD160 message digest of
    /// the file.
    pub fn rmd160(&self) -> Option<&[u8; 20]> {
        self.params.rmd160.as_ref().map(AsRef::as_ref)
    }

    /// `sha1|sha1digest` The FIPS 160-1 ("SHA-1") message digest of the file.
    pub fn sha1(&self) -> Option<&[u8; 20]> {
        self.params.sha1.as_ref().map(AsRef::as_ref)
    }

    /// `sha256|sha256digest` The FIPS 180-2 ("SHA-256") message digest of the
    /// file.
    pub fn sha256(&self) -> Option<&[u8; 32]> {
        self.params.sha256.as_ref()
    }

    /// `sha384|sha384digest` The FIPS 180-2 ("SHA-384") message digest of the
    /// file.
    pub fn sha384(&self) -> Option<&[u8; 48]> {
        self.params.sha384.as_ref().map(AsRef::as_ref)
    }

    /// `sha512|sha512digest` The FIPS 180-2 ("SHA-512") message digest of the
    /// file.
    pub fn sha512(&self) -> Option<&[u8; 64]> {
        self.params.sha512.as_ref().map(AsRef::as_ref)
    }

    /// `size` The size, in bytes, of the file.
    pub fn size(&self) -> Option<u64> {
        self.params.size
    }

    /// `time` The last modification time of the file.
    pub fn time(&self) -> Option<SystemTime> {
        self.params.time
    }

    /// `type` The type of the file.
    pub fn file_type(&self) -> Option<FileType> {
        self.params.file_type
    }

    /// The file owner as a numeric value.
    pub fn uid(&self) -> Option<u32> {
        self.params.uid
    }

    /// The file owner as a symbolic name.
    ///
    /// The name can be up to 32 chars and must match regex
    /// `[a-z_][a-z0-9_-]*[$]?`.
    pub fn uname(&self) -> Option<&[u8]> {
        self.params.uname.as_ref().map(AsRef::as_ref)
    }
}

/// All possible parameters to an entry.
///
/// All parameters are optional. `ignore`, `nochange` and `optional` all have no
/// value, and so `true` represets their presence.
#[derive(Default, Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
struct Params {
    /// `cksum` The checksum of the file using the default algorithm specified
    /// by the cksum(1) utility.
    pub checksum: Option<u64>,
    /// `device` The device number for *block* or *char* file types.
    pub device: Option<Box<Device>>,
    /// `contents` The full pathname of a file that holds the contents of this
    /// file.
    pub contents: Option<PathBuf>,
    /// `flags` The file flags as a symbolic name.
    pub flags: Option<Box<[u8]>>,
    /// `gid` The file group as a numeric value.
    pub gid: Option<u32>,
    /// `gname` The file group as a symbolic name.
    ///
    /// The name can be up to 32 chars and must match regex
    /// `[a-z_][a-z0-9_-]*[$]?`.
    pub gname: Option<Box<[u8]>>,
    /// `ignore` Ignore any file hierarchy below this line.
    pub ignore: bool,
    /// `inode` The inode number.
    pub inode: Option<u64>,
    /// `link` The target of the symbolic link when type=link.
    pub link: Option<PathBuf>,
    /// `md5|md5digest` The MD5 message digest of the file.
    pub md5: Option<u128>,
    /// `mode` The current file's permissions as a numeric (octal) or symbolic
    /// value.
    pub mode: Option<FileMode>,
    /// `nlink` The number of hard links the file is expected to have.
    pub nlink: Option<u64>,
    /// `nochange` Make sure this file or directory exists but otherwise ignore
    /// all attributes.
    pub no_change: bool,
    /// `optional` The file is optional; do not complain about the file if it is
    /// not in the file hierarchy.
    pub optional: bool,
    /// `resdevice` The "resident" device number of the file, e.g. the ID of the
    /// device that contains the file. Its format is the same as the one for
    /// `device`.
    pub resident_device: Option<Box<Device>>,
    /// `rmd160|rmd160digest|ripemd160digest` The RIPEMD160 message digest of
    /// the file.
    pub rmd160: Option<Box<[u8; 20]>>,
    /// `sha1|sha1digest` The FIPS 160-1 ("SHA-1") message digest of the file.
    pub sha1: Option<Box<[u8; 20]>>,
    /// `sha256|sha256digest` The FIPS 180-2 ("SHA-256") message digest of the
    /// file.
    pub sha256: Option<[u8; 32]>,
    /// `sha384|sha384digest` The FIPS 180-2 ("SHA-384") message digest of the
    /// file.
    pub sha384: Option<Box<[u8; 48]>>,
    /// `sha512|sha512digest` The FIPS 180-2 ("SHA-512") message digest of the
    /// file.
    pub sha512: Option<Box<[u8; 64]>>,
    /// `size` The size, in bytes, of the file.
    pub size: Option<u64>,
    /// `time` The last modification time of the file.
    pub time: Option<SystemTime>,
    /// `type` The type of the file.
    pub file_type: Option<FileType>,
    /// The file owner as a numeric value.
    pub uid: Option<u32>,
    /// The file owner as a symbolic name.
    ///
    /// The name can be up to 32 chars and must match regex
    /// `[a-z_][a-z0-9_-]*[$]?`.
    pub uname: Option<Box<[u8]>>,
}

impl Params {
    /// Helper method to set a number of parsed keywords.
    fn set_list<'a>(&mut self, keywords: impl Iterator<Item = Keyword<'a>>) {
        for keyword in keywords {
            self.set(keyword);
        }
    }

    /// Set a parameter from a parsed keyword.
    fn set(&mut self, keyword: Keyword<'_>) {
        match keyword {
            Keyword::Checksum(cksum) => self.checksum = Some(cksum),
            Keyword::DeviceRef(device) => self.device = Some(Box::new(device.to_device())),
            Keyword::Contents(contents) => {
                self.contents = Some(Path::new(OsStr::from_bytes(contents)).to_owned());
            }
            Keyword::Flags(flags) => self.flags = Some(flags.into()),
            Keyword::Gid(gid) => self.gid = Some(gid),
            Keyword::Gname(gname) => self.gname = Some(gname.into()),
            Keyword::Ignore => self.ignore = true,
            Keyword::Inode(inode) => self.inode = Some(inode),
            Keyword::Link(link) => {
                self.link = decode_escapes_path(&mut link.to_vec());
            }
            Keyword::Md5(md5) => self.md5 = Some(md5),
            Keyword::Mode(mode) => self.mode = Some(mode),
            Keyword::NLink(nlink) => self.nlink = Some(nlink),
            Keyword::NoChange => self.no_change = false,
            Keyword::Optional => self.optional = false,
            Keyword::ResidentDeviceRef(device) => {
                self.resident_device = Some(Box::new(device.to_device()));
            }
            Keyword::Rmd160(rmd160) => self.rmd160 = Some(Box::new(rmd160)),
            Keyword::Sha1(sha1) => self.sha1 = Some(Box::new(sha1)),
            Keyword::Sha256(sha256) => self.sha256 = Some(sha256),
            Keyword::Sha384(sha384) => self.sha384 = Some(Box::new(sha384)),
            Keyword::Sha512(sha512) => self.sha512 = Some(Box::new(sha512)),
            Keyword::Size(size) => self.size = Some(size),
            Keyword::Time(time) => self.time = Some(UNIX_EPOCH + time),
            Keyword::Type(ty) => self.file_type = Some(ty),
            Keyword::Uid(uid) => self.uid = Some(uid),
            Keyword::Uname(uname) => self.uname = Some(uname.into()),
        }
    }

    /*
    /// Empty this params list (better mem usage than creating a new one).
    fn clear(&mut self) {
        self.checksum = None;
        self.device = None;
        self.contents = None;
        self.flags = None;
        self.gid = None;
        self.gname = None;
        self.ignore = false;
        self.inode = None;
        self.link = None;
        self.md5 = None;
        self.mode = None;
        self.nlink = None;
        self.no_change = false;
        self.optional = false;
        self.resident_device = None;
        self.rmd160 = None;
        self.sha1 = None;
        self.sha256 = None;
        self.sha384 = None;
        self.sha512 = None;
        self.size = None;
        self.time = None;
        self.file_type = None;
        self.uid = None;
        self.uname = None;
    }
    */
}

impl fmt::Display for Params {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if let Some(v) = self.checksum {
            writeln!(f, "checksum: {v}")?;
        }
        if let Some(ref v) = self.device {
            writeln!(f, "device: {v:?}")?;
        }
        if let Some(ref v) = self.contents {
            writeln!(f, "contents: {}", v.display())?;
        }
        if let Some(ref v) = self.flags {
            writeln!(f, "flags: {v:?}")?;
        }
        if let Some(v) = self.gid
            && v != 0
        {
            writeln!(f, "gid: {v}")?;
        }
        if let Some(ref v) = self.gname {
            writeln!(f, "gname: {}", String::from_utf8_lossy(v))?;
        }
        if self.ignore {
            writeln!(f, "ignore")?;
        }
        if let Some(v) = self.inode {
            writeln!(f, "inode: {v}")?;
        }
        if let Some(ref v) = self.link {
            writeln!(f, "link: {}", v.display())?;
        }
        if let Some(ref v) = self.md5 {
            writeln!(f, "md5: {v:x}")?;
        }
        if let Some(ref v) = self.mode {
            writeln!(f, "mode: {v}")?;
        }
        if let Some(v) = self.nlink {
            writeln!(f, "nlink: {v}")?;
        }
        if self.no_change {
            writeln!(f, "no change")?;
        }
        if self.optional {
            writeln!(f, "optional")?;
        }
        if let Some(ref v) = self.resident_device {
            writeln!(f, "resident device: {v:?}")?;
        }
        if let Some(ref v) = self.rmd160 {
            write!(f, "rmd160: ")?;
            for ch in v.iter() {
                write!(f, "{ch:x}")?;
            }
            writeln!(f)?;
        }
        if let Some(ref v) = self.sha1 {
            write!(f, "sha1: ")?;
            for ch in v.iter() {
                write!(f, "{ch:x}")?;
            }
            writeln!(f)?;
        }
        if let Some(ref v) = self.sha256 {
            write!(f, "sha256: ")?;
            for ch in v {
                write!(f, "{ch:x}")?;
            }
            writeln!(f)?;
        }
        if let Some(ref v) = self.sha384 {
            write!(f, "sha384: ")?;
            for ch in v.iter() {
                write!(f, "{ch:x}")?;
            }
            writeln!(f)?;
        }
        if let Some(ref v) = self.sha512 {
            write!(f, "sha512: ")?;
            for ch in v.iter() {
                write!(f, "{ch:x}")?;
            }
            writeln!(f)?;
        }
        if let Some(v) = self.size {
            writeln!(f, "size: {v}")?;
        }
        if let Some(v) = self.time {
            writeln!(f, "modification time: {v:?}")?;
        }
        if let Some(v) = self.file_type {
            writeln!(f, "file type: {v}")?;
        }
        if let Some(v) = self.uid
            && v != 0
        {
            writeln!(f, "uid: {v}")?;
        }
        if let Some(ref v) = self.uname {
            writeln!(f, "uname: {}", String::from_utf8_lossy(v))?;
        }
        Ok(())
    }
}

/// A unix device.
///
/// The parsing for this could probably do with some work.
#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct Device {
    /// The device format.
    pub format: Format,
    /// The device major identifier.
    pub major: Vec<u8>,
    /// The device minor identifier.
    pub minor: Vec<u8>,
    /// The device subunit identifier, if applicable.
    pub subunit: Option<Vec<u8>>,
}

/// The error type for this crate.
///
/// There are 2 possible ways that this lib can fail - there can be a problem
/// parsing a record, or there can be a fault in the underlying reader.
#[derive(Debug)]
pub enum Error {
    /// There was an i/o error reading data from the reader.
    Io(io::Error),
    /// There was a problem parsing the records.
    Parser(ParserError),
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Io(..) => "an i/o error occured while reading the mtree",
            Self::Parser(..) => "an error occured while parsing the mtree",
        })
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(err) => Some(err),
            Self::Parser(err) => Some(err),
        }
    }
}

impl From<io::Error> for Error {
    fn from(from: io::Error) -> Self {
        Self::Io(from)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn test_wrapped_line_at_eof() {
        let data = r"# .
mtree_test \
size=581  \";
        let mtree = MTree::from_reader_with_empty_cwd(Cursor::new(data.as_bytes()));
        let entry = mtree.into_iter().next().unwrap().unwrap();
        let should = Entry {
            path: PathBuf::from("mtree_test"),
            params: Params {
                checksum: None,
                device: None,
                contents: None,
                flags: None,
                gid: None,
                gname: None,
                ignore: false,
                inode: None,
                link: None,
                md5: None,
                mode: None,
                nlink: None,
                no_change: false,
                optional: false,
                resident_device: None,
                rmd160: None,
                sha1: None,
                sha256: None,
                sha384: None,
                sha512: None,
                size: Some(581),
                time: None,
                file_type: None,
                uid: None,
                uname: None,
            },
        };
        assert_eq!(entry, should);
    }
}
