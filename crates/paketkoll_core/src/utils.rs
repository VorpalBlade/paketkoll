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
