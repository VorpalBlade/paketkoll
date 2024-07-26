//! Wrapping backend that performs disk cache

pub use from_archives::FromArchiveCache;
pub use original_files::OriginalFilesCache;

mod from_archives;
mod original_files;
mod utils;
