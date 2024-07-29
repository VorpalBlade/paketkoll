//! Utility misc stuff
use std::ffi::OsStr;
use std::os::unix::ffi::OsStrExt;
use std::time::Duration;

use crate::parser::ParserError;
use crate::parser::ParserResult;

/// Helper to parse a number from a slice of u8 in hexadecimal.
pub trait FromHex: Sized {
    /// Parse a number from a slice of u8 in hexadecimal.
    fn from_hex(input: &[u8]) -> ParserResult<Self>;
}

/// Helper to parse a number from a slice of u8 in decimal.
pub trait FromDec: Sized {
    /// Parse a number from a slice of u8 in decimal.
    fn from_dec(input: &[u8]) -> ParserResult<Self>;
}

macro_rules! impl_from_dec_uint {
    ($from:ty) => {
        impl FromDec for $from {
            fn from_dec(input: &[u8]) -> ParserResult<Self> {
                let mut acc: Self = 0;
                for (idx, i) in input.iter().enumerate() {
                    let val = from_dec_ch(*i).ok_or_else(|| {
                        format!(
                            r#"could not parse "{}" as a number, problem at char {}"#,
                            String::from_utf8_lossy(input),
                            idx
                        )
                    })?;
                    acc = acc
                        .checked_mul(10)
                        .ok_or_else(|| {
                            ParserError::from("could not parse integer - shift overflow".to_owned())
                        })?
                        .checked_add(val as $from)
                        .ok_or_else(|| {
                            ParserError::from(
                                "could not parse integer - addition overflow".to_owned(),
                            )
                        })?;
                }
                Ok(acc)
            }
        }
    };
}

impl_from_dec_uint!(u8);
impl_from_dec_uint!(u16);
impl_from_dec_uint!(u32);
impl_from_dec_uint!(u64);

macro_rules! impl_from_hex_arr {
    ($size:expr) => {
        impl FromHex for [u8; $size] {
            #[inline]
            fn from_hex(input: &[u8]) -> ParserResult<Self> {
                let mut result = [0; $size];
                match faster_hex::hex_decode(input, &mut result) {
                    Ok(_) => Ok(result),
                    Err(err) => Err(map_faster_hex_err(input, err)),
                }
            }
        }
    };
}

impl_from_hex_arr!(16);
impl_from_hex_arr!(20);
impl_from_hex_arr!(32);
impl_from_hex_arr!(48);
impl_from_hex_arr!(64);

#[cold]
fn map_faster_hex_err(input: &[u8], err: faster_hex::Error) -> ParserError {
    match err {
        faster_hex::Error::InvalidChar => format!(
            r#"input "{}" is not a valid hex string"#,
            String::from_utf8_lossy(input)
        )
        .into(),
        faster_hex::Error::InvalidLength(len) => format!(
            r#"input length ({}) must be twice the vec size, but it is not (in "{}")"#,
            len,
            String::from_utf8_lossy(input)
        )
        .into(),
        faster_hex::Error::Overflow => "Overflow on processing input".to_owned().into(),
    }
}

impl FromHex for u128 {
    /// Convert hex to u128
    ///
    /// # Panics
    ///
    /// The input length must be exactly 32.
    #[inline]
    fn from_hex(input: &[u8]) -> ParserResult<Self> {
        let mut dst = [0; 16];
        faster_hex::hex_decode(input, &mut dst).map_err(|e| map_faster_hex_err(input, e))?;
        Ok(u128::from_be_bytes(dst))
    }
}

/// If possible, quickly convert a character of a decimal number into a u8.
#[inline]
fn from_dec_ch(i: u8) -> Option<u8> {
    match i {
        b'0'..=b'9' => Some(i - b'0'),
        _ => None,
    }
}

/// If possible, quickly convert a character of a hexadecimal number into a u8.
#[inline]
fn from_oct_ch(i: u8) -> Option<u8> {
    match i {
        b'0'..=b'7' => Some(i - b'0'),
        _ => None,
    }
}

/// Convert a time of format `<seconds>.<nanos>` into a rust `Duration`.
pub fn parse_time(input: &[u8]) -> ParserResult<Duration> {
    let error = || -> ParserError {
        format!(
            r#"couldn't parse time from "{}""#,
            String::from_utf8_lossy(input)
        )
        .into()
    };
    let offset = memchr::memchr(b'.', input).ok_or_else(error)?;
    let sec = &input[..offset];
    let nano = &input[offset + 1..];
    let sec = u64::from_dec(sec)?;
    let nano = u32::from_dec(nano)?;
    Ok(Duration::new(sec, nano))
}

/// Spaces and other special characters are escaped, take care of that
pub fn decode_escapes_path(path: std::path::PathBuf) -> Option<std::path::PathBuf> {
    let path = path.into_os_string();
    let mut path = path.into_encoded_bytes();
    let path = decode_escapes(&mut path)?;

    // OsStr::from_bytes is Unix only. It is unlikely this will be used on Windows,
    // but provide a slower fallback implementation for that.
    //
    // We cannot use `OsStr::from_encoded_bytes_unchecked` safely here, since
    // it is possible the escape was not valid UTF-8, and we don't convert any
    // such string into valid WTF-8 (I wouldn't even know where to start).
    #[cfg(unix)]
    return Some(OsStr::from_bytes(path).into());
    #[cfg(not(unix))]
    return Some(String::from_utf8_lossy(path).into_owned().into());
}

/// Spaces and other special characters are escaped, take care of that
pub fn decode_escapes(buf: &mut [u8]) -> Option<&mut [u8]> {
    // Skip forward to the first escape character using a fast search.
    // Hopefully there will be nothing to do in the majority of cases
    let mut read_idx = memchr::memchr(b'\\', buf).unwrap_or(buf.len());
    let mut write_idx = read_idx;
    while read_idx < buf.len() {
        if buf[read_idx] == b'\\' {
            let ch = (from_oct_ch(buf[read_idx + 1])? << 6)
                | (from_oct_ch(buf[read_idx + 2])? << 3)
                | from_oct_ch(buf[read_idx + 3])?;
            buf[write_idx] = ch;
            read_idx += 3;
        } else {
            buf[write_idx] = buf[read_idx];
        }
        read_idx += 1;
        write_idx += 1;
    }
    Some(&mut buf[..write_idx])
}

/// A splitter using memchr to find the separators
#[derive(Debug)]
pub struct MemchrSplitter<'haystack> {
    inner: memchr::Memchr<'haystack>,
    haystack: &'haystack [u8],
    last: usize,
    done: bool,
}

impl<'haystack> MemchrSplitter<'haystack> {
    pub fn new(needle: u8, haystack: &'haystack [u8]) -> MemchrSplitter<'haystack> {
        Self {
            inner: memchr::memchr_iter(needle, haystack),
            haystack,
            last: 0,
            done: false,
        }
    }
}

impl<'haystack> Iterator for MemchrSplitter<'haystack> {
    type Item = &'haystack [u8];

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        // Code here is based on bstr::ByteSlice::split_str, but thanks to using memchr
        // instead of memmem this is much faster.
        match self.inner.next() {
            Some(start) => {
                let next = &self.haystack[self.last..start];
                self.last = start + 1;
                Some(next)
            }
            None => {
                if self.last >= self.haystack.len() {
                    if !self.done {
                        self.done = true;
                        Some(b"")
                    } else {
                        None
                    }
                } else {
                    let s = &self.haystack[self.last..];
                    self.last = self.haystack.len();
                    self.done = true;
                    Some(s)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::decode_escapes;
    use super::decode_escapes_path;
    use super::FromHex;

    #[test]
    fn test_parse_time() {
        assert_eq!(
            std::time::Duration::new(123, 456),
            super::parse_time(b"123.456").unwrap()
        );
    }

    #[test]
    fn test_decode_escapes_path() {
        assert_eq!(
            PathBuf::from("test"),
            decode_escapes_path(PathBuf::from("test")).unwrap()
        );
        assert_eq!(
            PathBuf::from("test test2"),
            decode_escapes_path(PathBuf::from("test\\040test2")).unwrap()
        );
    }

    #[test]
    fn test_decode_escapes() {
        assert_eq!(
            b"test",
            decode_escapes(b"test".to_owned().as_mut()).unwrap()
        );
        assert_eq!(
            b"test test2",
            decode_escapes(b"test\\040test2".to_owned().as_mut()).unwrap()
        );
    }

    #[test]
    fn test_hex_decode_u128() {
        assert_eq!(
            0x112233445566778899aabbccddeeff00,
            u128::from_hex(b"112233445566778899aabbccddeeff00").unwrap()
        );
    }

    #[test]
    fn test_hex_decode_array_64() {
        let expected: [u8; 64] = [
            0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee,
            0xff, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc,
            0xdd, 0xee, 0xff, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa,
            0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
            0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff, 0x00,
        ];
        assert_eq!(
            expected,
            <[u8; 64]>::from_hex(
                b"112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff00"
            )
            .unwrap()
        );
    }

    #[test]
    fn test_memchr_splitter() {
        let data = b"hello world";
        let mut splitter = super::MemchrSplitter::new(b' ', data);
        assert_eq!(splitter.next(), Some(b"hello".as_slice()));
        assert_eq!(splitter.next(), Some(b"world".as_slice()));
        assert_eq!(splitter.next(), None);

        let data = b"hello world ";
        let mut splitter = super::MemchrSplitter::new(b' ', data);
        assert_eq!(splitter.next(), Some(b"hello".as_slice()));
        assert_eq!(splitter.next(), Some(b"world".as_slice()));
        assert_eq!(splitter.next(), Some(b"".as_slice()));
        assert_eq!(splitter.next(), None);

        let data = b"";
        let mut splitter = super::MemchrSplitter::new(b' ', data);
        assert_eq!(splitter.next(), Some(b"".as_slice()));
        assert_eq!(splitter.next(), None);

        let data = b" ";
        let mut splitter = super::MemchrSplitter::new(b' ', data);
        assert_eq!(splitter.next(), Some(b"".as_slice()));
        assert_eq!(splitter.next(), Some(b"".as_slice()));
        assert_eq!(splitter.next(), None);
    }
}
