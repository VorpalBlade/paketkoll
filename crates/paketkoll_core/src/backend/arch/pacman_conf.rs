//! Parse pacman.conf

use compact_str::CompactString;
use eyre::OptionExt;
use eyre::WrapErr;
use std::io::Read;

/// Pacman configuration (or at least the parts we care about)
#[derive(Debug)]
pub(crate) struct PacmanConfig {
    pub(crate) root: CompactString,
    pub(crate) db_path: CompactString,
    pub(crate) cache_dir: CompactString,
}

impl PacmanConfig {
    pub(crate) fn new(file: &mut impl Read) -> eyre::Result<Self> {
        let parser = ini::Ini::read_from(file).wrap_err("Failed to open pacman.conf")?;
        let options: &ini::Properties = parser
            .section(Some("options"))
            .ok_or_eyre("Could not find options section in pacman.conf")?;

        Ok(Self {
            root: options.get("RootDir").unwrap_or("/").into(),
            db_path: options.get("DBPath").unwrap_or("/var/lib/pacman/").into(),
            cache_dir: options
                .get("CacheDir")
                .unwrap_or("/var/cache/pacman/pkg/")
                .into(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    #[test]
    fn test_pacman_config() {
        let file = indoc::indoc! {"
            [options]
            RootDir = /other
            DBPath = /dbpath
            # comment
            # Cachedir not set
        "};

        let config = PacmanConfig::new(&mut file.as_bytes()).unwrap();
        assert_eq!(config.root, "/other");
        assert_eq!(config.db_path, "/dbpath");
        assert_eq!(config.cache_dir, "/var/cache/pacman/pkg/");
    }
}
