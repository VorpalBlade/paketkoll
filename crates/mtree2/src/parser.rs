//! Stuff for parsing mtree files.
use crate::Device;
use crate::Error;
use crate::Params;
use crate::util::FromDec;
use crate::util::FromHex;
use crate::util::parse_time;
use core::str;
use smallvec::SmallVec;
use std::fmt;
use std::time::Duration;

/// An mtree file is a sequence of lines, each a semantic unit.
#[derive(Debug)]
pub enum MTreeLine<'a> {
    /// Blank lines are ignored.
    Blank,
    /// Lines starting with a '#' are ignored.
    Comment,
    /// Special commands (starting with '/') alter the behavior of later
    /// entries.
    Special(SpecialKind, SmallVec<[Keyword<'a>; 5]>),
    /// If the first word does not contain a '/', it is a file in the current
    /// directory.
    Relative(&'a [u8], SmallVec<[Keyword<'a>; 5]>),
    /// Change the current directory to the parent of the current directory.
    DotDot,
    /// If the first word does contain a '/', it is a file relative to the
    /// starting (not current) directory.
    Full(&'a [u8], SmallVec<[Keyword<'a>; 5]>),
    /// Wrapped line, accumulated parts
    Wrapped(&'a [u8]),
}

impl<'a> MTreeLine<'a> {
    pub fn from_bytes(input: &'a [u8]) -> ParserResult<Self> {
        let mut parts =
            crate::util::MemchrSplitter::new(b' ', input).filter(|word| !word.is_empty());
        // Blank
        let Some(first) = parts.next() else {
            return Ok(MTreeLine::Blank);
        };
        // Comment
        if first[0] == b'#' {
            return Ok(MTreeLine::Comment);
        }
        // DotDot
        if first == b".." {
            return Ok(MTreeLine::DotDot);
        }
        // the rest need params
        let mut params = SmallVec::new();
        for part in parts {
            let keyword = Keyword::from_bytes(part);
            debug_assert!(
                keyword.is_ok(),
                "could not parse bytes: {}",
                String::from_utf8_lossy(part)
            );
            if let Ok(keyword) = keyword {
                if matches!(keyword, Keyword::Wrapped) {
                    // Verify line ends with backslash
                    debug_assert_eq!(
                        input.last().copied(),
                        Some(b'\\'),
                        "Wrapped line must end with backslash"
                    );
                    return Ok(MTreeLine::Wrapped(input));
                }
                params.push(keyword);
            }
        }

        // Special
        if first[0] == b'/' {
            let kind = SpecialKind::from_bytes(&first[1..])?;
            Ok(MTreeLine::Special(kind, params))
        // Full
        } else if first.contains(&b'/') {
            Ok(MTreeLine::Full(first, params))
        } else {
            Ok(MTreeLine::Relative(first, params))
        }
    }
}

/// A command that alters the behavior of later commands.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpecialKind {
    /// Set a default for future lines.
    Set,
    /// Unset a default for future lines.
    Unset,
}

impl SpecialKind {
    fn from_bytes(input: &[u8]) -> ParserResult<Self> {
        Ok(match input {
            b"set" => Self::Set,
            b"unset" => Self::Unset,
            _ => {
                return Err(format!(
                    r#""{}" is not a special command"#,
                    String::from_utf8_lossy(input)
                )
                .into());
            }
        })
    }
}

/// Each entry may have one or more key word
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Keyword<'a> {
    /// `cksum` The checksum of the file using the default algorithm specified
    /// by the cksum(1) utility.
    // I'm pretty sure u32 is big enough, but I'm using u64 because I'm not sure that this is
    // guaranteed.
    Checksum(u64),
    /// `device` The device number for *block* or *char* file types.
    DeviceRef(DeviceRef<'a>),
    /// `contents` The full pathname of a file that holds the contents of this
    /// file.
    Contents(&'a [u8]),
    /// `flags` The file flags as a symbolic name.
    ///
    /// I think this is bsd-specific.
    Flags(&'a [u8]),
    /// `gid` The file group as a numeric value.
    Gid(u32),
    /// `gname` The file group as a symbolic name.
    Gname(&'a [u8]),
    /// `ignore` Ignore any file hierarchy below this line.
    Ignore,
    /// `inode` The inode number.
    Inode(u64),
    /// `link` The target of the symbolic link when type=link.
    Link(&'a [u8]),
    /// `md5|md5digest` The MD5 message digest of the file.
    Md5(u128),
    /// `mode` The current file's permissions as a numeric (octal) or symbolic
    /// value.
    Mode(FileMode),
    /// `nlink` The number of hard links the file is expected to have.
    NLink(u64),
    /// `nochange` Make sure this file or directory exists but otherwise ignore
    /// all attributes.
    NoChange,
    /// `optional` The file is optional; do not complain about the file if it is
    /// not in the file hierarchy.
    Optional,
    /// `resdevice` The "resident" device number of the file, e.g. the ID of the
    /// device that contains the file. Its format is the same as the one for
    /// `device`.
    ResidentDeviceRef(DeviceRef<'a>),
    /// `rmd160|rmd160digest|ripemd160digest` The RIPEMD160 message digest of
    /// the file.
    Rmd160([u8; 20]),
    /// `sha1|sha1digest` The FIPS 160-1 ("SHA-1") message digest of the file.
    Sha1([u8; 20]),
    /// `sha256|sha256digest` The FIPS 180-2 ("SHA-256") message digest of the
    /// file.
    Sha256([u8; 32]),
    /// `sha384|sha384digest` The FIPS 180-2 ("SHA-384") message digest of the
    /// file.
    Sha384([u8; 48]),
    /// `sha512|sha512digest` The FIPS 180-2 ("SHA-512") message digest of the
    /// file.
    Sha512([u8; 64]),
    /// `size` The size, in bytes, of the file.
    Size(u64),
    /// `time` The last modification time of the file, as a duration since the
    /// unix epoch.
    // The last modification time of the file, in seconds and nanoseconds. The value should
    // include a period character and exactly nine digits after the period.
    Time(Duration),
    /// `type` The type of the file.
    Type(FileType),
    /// The file owner as a numeric value.
    Uid(u32),
    /// The file owner as a symbolic name.
    Uname(&'a [u8]),
    /// Handle backslash wrapped Lines
    Wrapped,
}
impl<'a> Keyword<'a> {
    /// Parse a keyword with optional value.
    ///
    /// Input must be a non-empty slice
    fn from_bytes(input: &'a [u8]) -> ParserResult<Self> {
        fn next<'a>(field: &'static str, val: Option<&'a [u8]>) -> ParserResult<&'a [u8]> {
            val.ok_or_else(|| format!(r#""{field}" requires a parameter, none found"#).into())
        }
        debug_assert!(!input.len() > 0, "Input must be non-empty");
        let mut iter = input.splitn(2, |ch| *ch == b'=');
        // Unwrap cannot fail, as long as input is non-empty, guaranteed by the caller.
        let key = iter.next().expect("cannot fail");
        Ok(match key {
            b"cksum" => Keyword::Checksum(u64::from_dec(next("cksum", iter.next())?)?),
            b"device" => Keyword::DeviceRef(DeviceRef::from_bytes(next("devices", iter.next())?)?),
            b"contents" => Keyword::Contents(next("contents", iter.next())?),
            b"flags" => Keyword::Flags(next("flags", iter.next())?),
            b"gid" => Keyword::Gid(u32::from_dec(next("gid", iter.next())?)?),
            b"gname" => Keyword::Gname(next("gname", iter.next())?),
            b"ignore" => Keyword::Ignore,
            b"inode" => Keyword::Inode(u64::from_dec(next("inode", iter.next())?)?),
            b"link" => Keyword::Link(next("link", iter.next())?),
            b"md5" | b"md5digest" => {
                Keyword::Md5(u128::from_hex(next("md5|md5digest", iter.next())?)?)
            }
            b"mode" => Keyword::Mode(FileMode::from_bytes(next("mode", iter.next())?)?),
            b"nlink" => Keyword::NLink(u64::from_dec(next("nlink", iter.next())?)?),
            b"nochange" => Keyword::NoChange,
            b"optional" => Keyword::Optional,
            b"resdevice" => {
                Keyword::ResidentDeviceRef(DeviceRef::from_bytes(next("resdevice", iter.next())?)?)
            }
            b"rmd160" | b"rmd160digest" | b"ripemd160digest" => Keyword::Rmd160(
                <[u8; 20]>::from_hex(next("rmd160|rmd160digest|ripemd160digest", iter.next())?)?,
            ),
            b"sha1" | b"sha1digest" => {
                Keyword::Sha1(<[u8; 20]>::from_hex(next("sha1|sha1digest", iter.next())?)?)
            }
            b"sha256" | b"sha256digest" => Keyword::Sha256(<[u8; 32]>::from_hex(next(
                "sha256|sha256digest",
                iter.next(),
            )?)?),
            b"sha384" | b"sha384digest" => Keyword::Sha384(<[u8; 48]>::from_hex(next(
                "sha384|sha384digest",
                iter.next(),
            )?)?),
            b"sha512" | b"sha512digest" => Keyword::Sha512(<[u8; 64]>::from_hex(next(
                "sha512|sha512digest",
                iter.next(),
            )?)?),
            b"size" => Keyword::Size(u64::from_dec(next("size", iter.next())?)?),
            b"time" => Keyword::Time(parse_time(next("time", iter.next())?)?),
            b"type" => Keyword::Type(FileType::from_bytes(next("type", iter.next())?)?),
            b"uid" => Keyword::Uid(u32::from_dec(next("uid", iter.next())?)?),
            b"uname" => Keyword::Uname(next("uname", iter.next())?),
            b"\\" => Keyword::Wrapped,
            other => {
                return Err(format!(
                    r#""{}" is not a valid parameter key (in "{}")"#,
                    String::from_utf8_lossy(other),
                    String::from_utf8_lossy(input)
                )
                .into());
            }
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Hash)]
pub struct DeviceRef<'a> {
    /// The device format
    format: Format,
    /// The device major identifier
    major: &'a [u8],
    /// The device minor identifier
    minor: &'a [u8],
    /// The device subunit identifier, if applicable.
    subunit: Option<&'a [u8]>,
}

impl<'a> DeviceRef<'a> {
    /// Take ownership of the underlying data by copying
    pub fn to_device(&self) -> Device {
        Device {
            format: self.format,
            major: self.major.to_owned(),
            minor: self.minor.to_owned(),
            subunit: self.subunit.map(ToOwned::to_owned),
        }
    }

    fn from_bytes(input: &'a [u8]) -> ParserResult<Self> {
        let mut iter = input.splitn(4, |ch| *ch == b',');
        let format = Format::from_bytes(iter.next().ok_or_else(|| {
            format!(
                r#"could not read format from device "{}""#,
                String::from_utf8_lossy(input)
            )
        })?)?;
        let major = iter.next().ok_or_else(|| {
            format!(
                r#"could not read major field from device "{}""#,
                String::from_utf8_lossy(input)
            )
        })?;
        let minor = iter.next().ok_or_else(|| {
            format!(
                r#"could not read minor field from device "{}""#,
                String::from_utf8_lossy(input)
            )
        })?;
        // optional, so no '?'
        let subunit = iter.next();
        Ok(DeviceRef {
            format,
            major,
            minor,
            subunit,
        })
    }
}

/// The available device formats.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Format {
    Native,
    Bsd386,
    Bsd4,
    BsdOs,
    FreeBsd,
    Hpux,
    Isc,
    Linux,
    NetBsd,
    Osf1,
    Sco,
    Solaris,
    SunOs,
    Svr3,
    Svr4,
    Ultrix,
}

impl Format {
    fn from_bytes(bytes: &[u8]) -> ParserResult<Self> {
        Ok(match bytes {
            b"native" => Self::Native,
            b"386bsd" => Self::Bsd386,
            b"4bsd" => Self::Bsd4,
            b"bsdos" => Self::BsdOs,
            b"freebsd" => Self::FreeBsd,
            b"hpux" => Self::Hpux,
            b"isc" => Self::Isc,
            b"linux" => Self::Linux,
            b"netbsd" => Self::NetBsd,
            b"osf1" => Self::Osf1,
            b"sco" => Self::Sco,
            b"solaris" => Self::Solaris,
            b"sunos" => Self::SunOs,
            b"svr3" => Self::Svr3,
            b"svr4" => Self::Svr4,
            b"ultrix" => Self::Ultrix,
            other => {
                return Err(format!(
                    r#""{}" is not a valid format"#,
                    String::from_utf8_lossy(other)
                )
                .into());
            }
        })
    }
}

#[test]
fn test_format_from_butes() {
    for (input, res) in vec![
        (&b"native"[..], Format::Native),
        (&b"386bsd"[..], Format::Bsd386),
        (&b"4bsd"[..], Format::Bsd4),
        (&b"bsdos"[..], Format::BsdOs),
        (&b"freebsd"[..], Format::FreeBsd),
        (&b"hpux"[..], Format::Hpux),
        (&b"isc"[..], Format::Isc),
        (&b"linux"[..], Format::Linux),
        (&b"netbsd"[..], Format::NetBsd),
        (&b"osf1"[..], Format::Osf1),
        (&b"sco"[..], Format::Sco),
        (&b"solaris"[..], Format::Solaris),
        (&b"sunos"[..], Format::SunOs),
        (&b"svr3"[..], Format::Svr3),
        (&b"svr4"[..], Format::Svr4),
        (&b"ultrix"[..], Format::Ultrix),
    ] {
        assert_eq!(Format::from_bytes(input), Ok(res));
    }
}

/// The type of an entry.
///
/// In an mtree file, entries can be files, directories, and some other special
/// unix types like block/character devices.
#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FileType {
    /// A unix block device.
    BlockDevice,
    /// A unix character device.
    CharacterDevice,
    /// A directory.
    Directory,
    /// A unix fifo (named pipe), useful for IPC.
    Fifo,
    /// A standard file.
    File,
    /// A symbolic link.
    SymbolicLink,
    /// A unix socket.
    Socket,
}

impl FileType {
    fn from_bytes(input: &[u8]) -> ParserResult<Self> {
        Ok(match input {
            b"block" => Self::BlockDevice,
            b"char" => Self::CharacterDevice,
            b"dir" => Self::Directory,
            b"fifo" => Self::Fifo,
            b"file" => Self::File,
            b"link" => Self::SymbolicLink,
            b"socket" => Self::Socket,
            _ => {
                return Err(format!(
                    r#""{}" is not a valid file type"#,
                    String::from_utf8_lossy(input)
                )
                .into());
            }
        })
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::BlockDevice => "block",
            Self::CharacterDevice => "char",
            Self::Directory => "dir",
            Self::Fifo => "fifo",
            Self::File => "file",
            Self::SymbolicLink => "link",
            Self::Socket => "socket",
        }
    }
}

impl fmt::Display for FileType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[test]
fn test_type_from_bytes() {
    for (input, res) in [
        (&b"block"[..], FileType::BlockDevice),
        (&b"char"[..], FileType::CharacterDevice),
        (&b"dir"[..], FileType::Directory),
        (&b"fifo"[..], FileType::Fifo),
        (&b"file"[..], FileType::File),
        (&b"link"[..], FileType::SymbolicLink),
        (&b"socket"[..], FileType::Socket),
    ] {
        assert_eq!(FileType::from_bytes(input), Ok(res));
    }
    assert!(FileType::from_bytes(&b"other"[..]).is_err());
}

bitflags::bitflags! {
    /// Unix file permissions.
    #[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
    pub struct Perms: u8 {
        /// Entity has read access.
        const READ = 0b100;
        /// Entity has write access.
        const WRITE = 0b010;
        /// Entity has execute access.
        const EXECUTE = 0b001;
    }
}

impl fmt::Display for Perms {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.contains(Self::READ) {
            f.write_str("r")?;
        } else {
            f.write_str("-")?;
        }
        if self.contains(Self::WRITE) {
            f.write_str("w")?;
        } else {
            f.write_str("-")?;
        }
        if self.contains(Self::EXECUTE) {
            f.write_str("x")?;
        } else {
            f.write_str("-")?;
        }
        Ok(())
    }
}

/// The file/dir permissions for owner/group/everyone else.
#[derive(Debug, Copy, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct FileMode {
    mode: u32,
}

impl FileMode {
    fn from_bytes(input: &[u8]) -> ParserResult<Self> {
        // file mode can either be symbolic, or octal. For now only support octal
        if input.len() > 4 {
            return Err(format!(
                r#"mode value must be 4 or less octal chars, found "{}""#,
                String::from_utf8_lossy(input)
            )
            .into());
        }
        Ok(Self {
            mode: u32::from_str_radix(
                std::str::from_utf8(input)
                    .map_err(|err| ParserError(format!("failed to parse mode value: {err}")))?,
                8,
            )
            .map_err(|err| ParserError(format!("failed to parse mode as integer: {err}")))?,
        })
    }

    /// Executable files with this bit set will
    /// run with effective uid set to the uid of the file owner.
    pub fn setuid(self) -> bool {
        self.mode & 0o4000 != 0
    }

    /// Executable files with this bit set will
    /// run with effective gid set to the gid of the file owner.
    pub fn setgid(self) -> bool {
        self.mode & 0o2000 != 0
    }

    /// Is the sticky bit set?
    pub fn sticky(self) -> bool {
        self.mode & 0o1000 != 0
    }

    /// The permissions for the owner of the file.
    pub fn owner(self) -> Perms {
        const MASK: u32 = 0o700;
        Perms::from_bits_truncate(((self.mode & MASK) >> 6) as u8)
    }

    /// The permissions for everyone who is not the owner, but in the group.
    pub fn group(self) -> Perms {
        const MASK: u32 = 0o070;
        Perms::from_bits_truncate(((self.mode & MASK) >> 3) as u8)
    }

    /// The permissions for everyone who is not the owner and not in the group.
    pub fn other(self) -> Perms {
        const MASK: u32 = 0o007;
        Perms::from_bits_truncate((self.mode & MASK) as u8)
    }
}

/// Convert to u32 for compatibility with standard library
impl From<FileMode> for u32 {
    fn from(value: FileMode) -> Self {
        value.mode
    }
}

impl fmt::Display for FileMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}{}{}", self.owner(), self.group(), self.other())
    }
}

impl fmt::Octal for FileMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:o}{:o}{:o}", self.owner(), self.group(), self.other())
    }
}

pub(crate) type ParserResult<T> = Result<T, ParserError>;

/// An error occurred during parsing a record.
///
/// This currently just gives an error report at the moment.
#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct ParserError(pub String);

impl From<String> for ParserError {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl fmt::Display for ParserError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ParserError {}
