//! More or less freestanding utility function for konfigkoll.
//!
//! Not a public API, but does follow semver.

pub mod line_edit;
mod utils;

pub use utils::safe_path_join;
