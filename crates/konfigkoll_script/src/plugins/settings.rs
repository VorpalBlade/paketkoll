//! Konfigkoll settings

use ahash::AHashSet;
use anyhow::Context;
use camino::Utf8PathBuf;
use parking_lot::Mutex;
use rune::ContextError;
use rune::Module;
use std::str::FromStr;

/// Configuration of how konfigkoll should behave.
#[derive(Debug, rune::Any)]
#[rune(item = ::settings)]
pub struct Settings {
    file_backend: Mutex<Option<paketkoll_types::backend::Backend>>,
    enabled_pkg_backends: Mutex<AHashSet<paketkoll_types::backend::Backend>>,
    /// Configuration files (such as `/etc/passwd`) that should be applied early,
    /// before installing packages.
    /// This is useful to assign the same IDs instead of auto assignment
    early_configs: Mutex<AHashSet<Utf8PathBuf>>,
    /// Diff tool to use for comparing files. Default is `diff`.
    diff: Mutex<Vec<String>>,
    /// Pager to use, default is to use $PAGER and fall back to `less`
    pager: Mutex<Vec<String>>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            file_backend: Mutex::new(None),
            enabled_pkg_backends: Mutex::new(AHashSet::new()),
            early_configs: Mutex::new(AHashSet::new()),
            diff: Mutex::new(vec!["diff".into(), "-Naur".into()]),
            pager: Mutex::new(vec![]),
        }
    }
}

/// Rust API
impl Settings {
    pub fn is_file_backend_enabled(&self, backend: paketkoll_types::backend::Backend) -> bool {
        let guard = self.file_backend.lock();
        *guard == Some(backend)
    }

    pub fn is_pkg_backend_enabled(&self, backend: paketkoll_types::backend::Backend) -> bool {
        let guard = self.enabled_pkg_backends.lock();
        guard.contains(&backend)
    }

    pub fn file_backend(&self) -> Option<paketkoll_types::backend::Backend> {
        let guard = self.file_backend.lock();
        *guard
    }

    pub fn enabled_pkg_backends(&self) -> impl Iterator<Item = paketkoll_types::backend::Backend> {
        let guard = self.enabled_pkg_backends.lock();
        let v: Vec<_> = guard.iter().cloned().collect();
        v.into_iter()
    }

    pub fn early_configs(&self) -> impl Iterator<Item = Utf8PathBuf> {
        let guard = self.early_configs.lock();
        let v: Vec<_> = guard.iter().cloned().collect();
        v.into_iter()
    }

    /// Get diff tool to use
    pub fn diff(&self) -> Vec<String> {
        let guard = self.diff.lock();
        guard.clone()
    }

    /// Get preferred pager to use
    pub fn pager(&self) -> Vec<String> {
        let guard = self.pager.lock();
        if guard.len() >= 1 {
            guard.clone()
        } else {
            vec![std::env::var("PAGER").ok().unwrap_or_else(|| "less".into())]
        }
    }
}

/// Rune API
impl Settings {
    /// Set a package manager as the data source and target for file system checks.
    ///
    /// Unlike package manager backends, there can only be one of these (otherwise
    /// the semantics would get confusing regarding which files are managed by which
    /// tool).
    ///
    /// Valid values are:
    /// * "pacman" (Arch Linux and derivatives)
    /// * "apt" (Debian and derivatives)
    ///
    /// This will return an error on other values.
    #[rune::function]
    pub fn set_file_backend(&self, name: &str) -> anyhow::Result<()> {
        let backend = paketkoll_types::backend::Backend::from_str(name)
            .with_context(|| format!("Unknown backend {name}"))?;
        let mut guard = self.file_backend.lock();
        if guard.is_some() {
            tracing::warn!("File backend was set more than once");
        }
        *guard = Some(backend);

        Ok(())
    }

    /// Enable a package manager or other backend as a data source and target for package operations.
    ///
    /// Multiple ones can be enabled at the same time (typically flatpak + native package manager).
    ///
    /// Valid values are:
    /// * "pacman" (Arch Linux and derivatives)
    /// * "apt" (Debian and derivatives)
    /// * "flatpak" (Flatpak)
    ///
    /// This will return an error on other values.
    #[rune::function]
    pub fn enable_pkg_backend(&self, name: &str) -> anyhow::Result<()> {
        let backend = paketkoll_types::backend::Backend::from_str(name)
            .with_context(|| format!("Unknown backend {name}"))?;

        let before = self.enabled_pkg_backends.lock().insert(backend);

        if !before {
            tracing::warn!("Package backend {name} was enabled more than once");
        }

        Ok(())
    }

    /// Add a configuration file that should be applied early (before package installation).
    /// This is useful for files like `/etc/passwd` to assign the same IDs instead
    /// of auto assignment at package installation.
    #[rune::function]
    pub fn early_config(&self, path: &str) {
        let before = self.early_configs.lock().insert(path.into());

        if !before {
            tracing::warn!("Early config {path} was added more than once");
        }
    }

    /// Set the diff tool to use for comparing files.
    ///
    /// Default is `diff`.
    #[rune::function]
    pub fn set_diff(&self, cmd: Vec<String>) {
        let mut guard = self.diff.lock();
        *guard = cmd;
    }

    /// Set the pager to use for viewing files.
    ///
    /// Default is to use `$PAGER` and fall back to `less`.
    #[rune::function]
    pub fn set_pager(&self, cmd: Vec<String>) {
        let mut guard = self.pager.lock();
        *guard = cmd;
    }
}

#[rune::module(::settings)]
/// Settings of how konfigkoll should behave.
pub(crate) fn module() -> Result<Module, ContextError> {
    let mut m = Module::from_meta(self::module_meta)?;
    m.ty::<Settings>()?;
    m.function_meta(Settings::set_file_backend)?;
    m.function_meta(Settings::enable_pkg_backend)?;
    m.function_meta(Settings::early_config)?;
    m.function_meta(Settings::set_diff)?;
    m.function_meta(Settings::set_pager)?;
    Ok(m)
}