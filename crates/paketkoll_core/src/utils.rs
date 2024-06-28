//! Various utility functions

use crate::types::Checksum;
use anyhow::Context;
use std::io::ErrorKind;

/// Mask out the bits of the mode that are actual permissions
pub(crate) const MODE_MASK: u32 = 0o7777;

#[allow(dead_code)]
#[cfg(feature = "__sha256")]
pub(crate) fn sha256_readable(reader: &mut impl std::io::Read) -> anyhow::Result<Checksum> {
    let mut buffer = [0; 16 * 1024];
    let mut hasher = ring::digest::Context::new(&ring::digest::SHA256);
    loop {
        match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(n) => {
                hasher.update(&buffer[..n]);
            }
            Err(ref e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => Err(e)?,
        }
    }
    let digest = hasher.finish();
    Ok(Checksum::Sha256(
        digest
            .as_ref()
            .try_into()
            .context("Invalid digest length")?,
    ))
}

#[allow(dead_code)]
#[cfg(feature = "__sha256")]
pub(crate) fn sha256_buffer(contents: &[u8]) -> anyhow::Result<Checksum> {
    let mut hasher = ring::digest::Context::new(&ring::digest::SHA256);
    hasher.update(contents);
    let digest = hasher.finish();
    Ok(Checksum::Sha256(
        digest
            .as_ref()
            .try_into()
            .context("Invalid digest length")?,
    ))
}

/// Serializes `buffer` to a lowercase hex string.
#[cfg(feature = "serde")]
pub(crate) fn buffer_to_hex<T, S>(buffer: &T, serializer: S) -> Result<S::Ok, S::Error>
where
    T: AsRef<[u8]>,
    S: serde::Serializer,
{
    let buffer = buffer.as_ref();
    // We only use this for checksum, so small buffers. On the stack it goes:
    let mut buf = [0u8; 128];
    let s = faster_hex::hex_encode(buffer, &mut buf)
        .expect("This shouldn't fail on the data we use it for");
    serializer.serialize_str(s[0..buffer.len() * 2].as_ref())
}
