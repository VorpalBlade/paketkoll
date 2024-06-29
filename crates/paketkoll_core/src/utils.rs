//! Various utility functions

use anyhow::Context;
use paketkoll_types::files::Checksum;
use std::{io::ErrorKind, os::unix::process::ExitStatusExt};

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

/// Helper to do a generic package manager transaction
pub(crate) fn package_manager_transaction(
    program_name: &str,
    mode: &str,
    pkg_list: &[compact_str::CompactString],
    ask_confirmation: Option<&str>,
) -> anyhow::Result<()> {
    let mut apt_get = std::process::Command::new(program_name);
    apt_get.arg(mode);
    if let Some(flag) = ask_confirmation {
        apt_get.arg(flag);
    }
    for pkg in pkg_list {
        apt_get.arg(pkg.as_str());
    }
    let status = apt_get
        .status()
        .with_context(|| format!("Failed to execute {program_name}"))?;
    if !status.success() {
        match status.code() {
            Some(code) => anyhow::bail!("{program_name} failed with exit code {code}"),
            _ => anyhow::bail!("{program_name} failed with signal {:?}", status.signal()),
        }
    }
    Ok(())
}
