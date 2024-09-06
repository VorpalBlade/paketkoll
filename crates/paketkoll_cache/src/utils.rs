//! Utility functions

use compact_str::format_compact;
use compact_str::CompactString;
use paketkoll_types::intern::Interner;
use paketkoll_types::package::PackageInterned;

/// Format a package for use in cache keys
pub(crate) fn format_package(pkg: &PackageInterned, interner: &Interner) -> CompactString {
    format_compact!(
        "{}:{}:{}:{}",
        pkg.name.as_str(interner),
        pkg.architecture
            .map(|v| v.as_str(interner))
            .unwrap_or_default(),
        pkg.version,
        pkg.ids
            .iter()
            .map(|v| v.as_str(interner))
            .collect::<Vec<_>>()
            .join("#")
    )
}
