//! Code to save config

use std::io::Write;

use anyhow::Context;
use camino::Utf8Path;
use konfigkoll_core::utils::safe_path_join;
use konfigkoll_types::FileContents;

/// Copy files to the config directory, under the "files/".
pub(crate) fn file_data_saver(
    files_path: &Utf8Path,
    path: &Utf8Path,
    contents: &FileContents,
) -> Result<(), anyhow::Error> {
    tracing::info!("Saving file data for {}", path);
    let full_path = safe_path_join(files_path, path);
    std::fs::create_dir_all(full_path.parent().with_context(|| {
        format!("Impossible error: joined path should always below config dir: {full_path}")
    })?)?;
    match contents {
        FileContents::Literal { checksum: _, data } => {
            let mut file = std::fs::File::create(&full_path)?;
            file.write_all(data)?;
        }
        FileContents::FromFile { checksum: _, path } => {
            std::fs::copy(path, &full_path)?;
        }
    }
    Ok(())
}

pub(crate) fn noop_file_data_saver(path: &Utf8Path) -> Result<(), anyhow::Error> {
    tracing::info!("Would save file data for {}", path);
    Ok(())
}