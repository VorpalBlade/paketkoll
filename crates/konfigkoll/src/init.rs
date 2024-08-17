//! Set up a new configuration directory from the template

use camino::Utf8Path;
use eyre::Context;

/// Set up a new configuration directory from the template
pub(crate) fn init_directory(config_path: &Utf8Path) -> eyre::Result<()> {
    std::fs::create_dir_all(config_path).context("Failed to create config directory")?;
    std::fs::create_dir_all(config_path.join("files"))?;

    // Create skeleton main script
    let main_script = config_path.join("main.rn");
    if !main_script.exists() {
        std::fs::write(&main_script, include_bytes!("../data/template/main.rn"))?;
    }
    // Create skeleton unsorted script
    let unsorted_script = config_path.join("unsorted.rn");
    if !unsorted_script.exists() {
        std::fs::write(
            &unsorted_script,
            include_bytes!("../data/template/unsorted.rn"),
        )?;
    }
    // Gitignore
    let gitignore = config_path.join(".gitignore");
    if !gitignore.exists() {
        std::fs::write(&gitignore, include_bytes!("../data/template/_gitignore"))?;
    }

    // Add an empty Rune.toml
    let runetoml = config_path.join("Rune.toml");
    if !runetoml.exists() {
        std::fs::write(&runetoml, b"")?;
    }

    Ok(())
}
