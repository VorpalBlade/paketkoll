//! Checksum utilities

use std::io::ErrorKind;

use anyhow::Context;

use paketkoll_types::files::Checksum;

pub fn sha256_readable(reader: &mut impl std::io::Read) -> anyhow::Result<Checksum> {
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

pub fn sha256_buffer(contents: &[u8]) -> Checksum {
    let mut hasher = ring::digest::Context::new(&ring::digest::SHA256);
    hasher.update(contents);
    let digest = hasher.finish();
    Checksum::Sha256(digest.as_ref().try_into().expect("Invalid digest length"))
}
