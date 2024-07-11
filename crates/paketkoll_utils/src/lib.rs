//! Internal helper crate for paketkoll & konfigkoll.
//!
//! Not for external usage. No stability guarantees whatsoever.

pub mod checksum;

/// Mask out the bits of the mode that are actual permissions
pub const MODE_MASK: u32 = 0o7777;
