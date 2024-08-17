//! Checksum utilities

use eyre::WrapErr;
use paketkoll_types::files::Checksum;
use std::io::ErrorKind;

pub fn sha256_readable(reader: &mut impl std::io::Read) -> eyre::Result<Checksum> {
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
            .wrap_err("Invalid digest length")?,
    ))
}

pub fn sha256_buffer(contents: &[u8]) -> Checksum {
    let mut hasher = ring::digest::Context::new(&ring::digest::SHA256);
    hasher.update(contents);
    let digest = hasher.finish();
    Checksum::Sha256(digest.as_ref().try_into().expect("Invalid digest length"))
}
