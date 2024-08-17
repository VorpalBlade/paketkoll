//! Commands to change the configuration
//!
//! These are the important ones, the ones that describe how the system should
//! be changed.

use super::error::KResult;
use super::settings::Settings;
use crate::Phase;
use ahash::AHashSet;
use camino::Utf8PathBuf;
use compact_str::CompactString;
use eyre::Context;
use konfigkoll_types::FileContents;
use konfigkoll_types::FsInstruction;
use konfigkoll_types::FsOp;
use konfigkoll_types::FsOpDiscriminants;
use konfigkoll_types::PkgIdent;
use konfigkoll_types::PkgInstruction;
use konfigkoll_types::PkgInstructions;
use konfigkoll_types::PkgOp;
use konfigkoll_utils::safe_path_join;
use paketkoll_types::backend::Backend;
use paketkoll_types::files::Mode;
use rune::ContextError;
use rune::Module;
use rune::Value;
use std::str::FromStr;
use std::sync::Arc;

#[derive(Debug, Clone, rune::Any)]
#[rune(item = ::command)]
/// The changes to apply to the system.
///
/// This is what will be compared to the installed system
pub struct Commands {
    /// The current phase
    pub(crate) phase: Phase,
    /// Base path to files directory
    pub(crate) base_files_path: Utf8PathBuf,
    /// Set of file system ignores
    pub fs_ignores: AHashSet<CompactString>,
    /// Queue of file system instructions
    pub fs_actions: Vec<FsInstruction>,
    /// Queue of package instructions
    pub package_actions: PkgInstructions,
    /// Settings
    settings: Arc<Settings>,
}

/// Rust API
impl Commands {
    pub(crate) fn new(base_files_path: Utf8PathBuf, settings: Arc<Settings>) -> Self {
        Self {
            phase: Phase::SystemDiscovery,
            base_files_path,
            fs_ignores: AHashSet::new(),
            fs_actions: Vec::new(),
            package_actions: PkgInstructions::new(),
            settings,
        }
    }

    /// Get the contents of a previously set file
    pub(crate) fn file_contents(&self, path: &str) -> Option<&FileContents> {
        self.fs_actions
            .iter()
            .rfind(|i| {
                i.path == path && FsOpDiscriminants::from(&i.op) == FsOpDiscriminants::CreateFile
            })
            .map(|i| match &i.op {
                FsOp::CreateFile(contents) => contents,
                _ => unreachable!(),
            })
    }

    fn verify_path(&self, path: &str) -> eyre::Result<()> {
        if path.contains("..") {
            return Err(eyre::eyre!("Path {} contains '..'", path));
        }
        if !path.starts_with('/') {
            return Err(eyre::eyre!("Path {} is not absolute", path));
        }
        Ok(())
    }
}

/// Rune API
impl Commands {
    /// Ignore a path, preventing it from being scanned for differences
    #[rune::function(keep)]
    pub fn ignore_path(&mut self, ignore: &str) -> KResult<()> {
        if self.phase != Phase::Ignores {
            return Err(eyre::eyre!("Can only ignore paths during the 'ignores' phase").into());
        }
        if !self.fs_ignores.insert(ignore.into()) {
            tracing::warn!("Ignoring path '{}' multiple times", ignore);
        }
        Ok(())
    }

    /// Install a package with the given package manager.
    ///
    /// If the package manager isn't enabled, this will be a no-op.
    #[rune::function(keep)]
    pub fn add_pkg(&mut self, package_manager: &str, identifier: &str) -> KResult<()> {
        if self.phase < Phase::ScriptDependencies {
            return Err(eyre::eyre!(
                "Can only add packages during the 'script_dependencies' or 'main' phases"
            )
            .into());
        }
        let backend = Backend::from_str(package_manager).context("Invalid backend")?;
        if !self.settings.is_pkg_backend_enabled(backend) {
            tracing::debug!("Skipping disabled package manager {}", package_manager);
            return Ok(());
        }
        if self
            .package_actions
            .insert(
                PkgIdent {
                    package_manager: backend,
                    identifier: identifier.into(),
                },
                PkgInstruction {
                    op: PkgOp::Install,
                    comment: None,
                },
            )
            .is_some()
        {
            tracing::warn!("Multiple actions for package '{package_manager}:{identifier}'",);
        }
        Ok(())
    }

    /// Remove a package with the given package manager.
    ///
    /// If the package manager isn't enabled, this will be a no-op.
    #[rune::function(keep)]
    pub fn remove_pkg(&mut self, package_manager: &str, identifier: &str) -> KResult<()> {
        if self.phase < Phase::ScriptDependencies {
            return Err(eyre::eyre!(
                "Can only add packages during the 'script_dependencies' or 'main' phases"
            )
            .into());
        }
        let backend = Backend::from_str(package_manager).context("Invalid backend")?;
        if !self.settings.is_file_backend_enabled(backend) {
            tracing::debug!("Skipping disabled package manager {}", package_manager);
            return Ok(());
        }
        if self
            .package_actions
            .insert(
                PkgIdent {
                    package_manager: backend,
                    identifier: identifier.into(),
                },
                PkgInstruction {
                    op: PkgOp::Uninstall,
                    comment: None,
                },
            )
            .is_some()
        {
            tracing::warn!("Multiple actions for package '{package_manager}:{identifier}'",);
        }
        Ok(())
    }

    /// Remove a path
    #[rune::function(keep)]
    pub fn rm(&mut self, path: &str) -> KResult<()> {
        if self.phase != Phase::Main {
            return Err(
                eyre::eyre!("File system actions are only possible in the 'main' phase").into(),
            );
        }
        self.fs_actions.push(FsInstruction {
            op: FsOp::Remove,
            path: path.into(),
            comment: None,
            pkg: None,
        });
        Ok(())
    }

    /// Check if a file exists in the `files/` subdirectory to the configuration
    #[rune::function(keep)]
    pub fn has_source_file(&self, path: &str) -> bool {
        let path = safe_path_join(&self.base_files_path, path.into());
        path.exists()
    }

    /// Create a file with the given contents
    #[rune::function(keep)]
    pub fn copy(&mut self, path: &str) -> KResult<()> {
        self.copy_from(path, path)
    }

    /// Create a file with the given contents (renaming the file in the process)
    ///
    /// The rename is useful to copy a file to a different location (e.g.
    /// `etc/fstab.hostname` to `etc/fstab`)
    #[rune::function(keep)]
    pub fn copy_from(&mut self, path: &str, src: &str) -> KResult<()> {
        if self.phase != Phase::Main {
            return Err(
                eyre::eyre!("File system actions are only possible in the 'main' phase").into(),
            );
        }
        self.verify_path(path)?;
        self.verify_path(src)?;
        let contents = FileContents::from_file(&safe_path_join(&self.base_files_path, src.into()));
        let contents = match contents {
            Ok(v) => v,
            Err(e) => {
                tracing::error!("Failed to read file contents for '{}': {}", path, e);
                return Err(
                    eyre::eyre!("Failed to read file contents for '{}': {}", path, e).into(),
                );
            }
        };
        self.fs_actions.push(FsInstruction {
            op: FsOp::CreateFile(contents),
            path: path.into(),
            comment: None,
            pkg: None,
        });
        Ok(())
    }

    /// Create a symlink
    #[rune::function(keep)]
    pub fn ln(&mut self, path: &str, target: &str) -> KResult<()> {
        if self.phase != Phase::Main {
            return Err(
                eyre::eyre!("File system actions are only possible in the 'main' phase").into(),
            );
        }
        self.verify_path(path)?;
        self.fs_actions.push(FsInstruction {
            op: FsOp::CreateSymlink {
                target: target.into(),
            },
            path: path.into(),
            comment: None,
            pkg: None,
        });
        Ok(())
    }

    /// Create a file with the given contents
    #[rune::function(keep)]
    pub fn write(&mut self, path: &str, contents: &[u8]) -> KResult<()> {
        if self.phase != Phase::Main {
            return Err(
                eyre::eyre!("File system actions are only possible in the 'main' phase").into(),
            );
        }
        self.verify_path(path)?;
        self.fs_actions.push(FsInstruction {
            op: FsOp::CreateFile(FileContents::from_literal(contents.into())),
            path: path.into(),
            comment: None,
            pkg: None,
        });
        Ok(())
    }

    /// Create a directory
    #[rune::function(keep)]
    pub fn mkdir(&mut self, path: &str) -> KResult<()> {
        if self.phase != Phase::Main {
            return Err(
                eyre::eyre!("File system actions are only possible in the 'main' phase").into(),
            );
        }
        self.verify_path(path)?;
        self.fs_actions.push(FsInstruction {
            op: FsOp::CreateDirectory,
            path: path.into(),
            comment: None,
            pkg: None,
        });
        Ok(())
    }

    /// Change file owner
    #[rune::function(keep)]
    pub fn chown(&mut self, path: &str, owner: &str) -> KResult<()> {
        if self.phase != Phase::Main {
            return Err(
                eyre::eyre!("File system actions are only possible in the 'main' phase").into(),
            );
        }
        self.verify_path(path)?;
        self.fs_actions.push(FsInstruction {
            op: FsOp::SetOwner {
                owner: owner.into(),
            },
            path: path.into(),
            comment: None,
            pkg: None,
        });
        Ok(())
    }

    /// Change file group
    #[rune::function(keep)]
    pub fn chgrp(&mut self, path: &str, group: &str) -> KResult<()> {
        if self.phase != Phase::Main {
            return Err(
                eyre::eyre!("File system actions are only possible in the 'main' phase").into(),
            );
        }
        self.verify_path(path)?;
        self.fs_actions.push(FsInstruction {
            op: FsOp::SetGroup {
                group: group.into(),
            },
            path: path.into(),
            comment: None,
            pkg: None,
        });
        Ok(())
    }

    /// Change file mode
    #[rune::function(keep)]
    pub fn chmod(&mut self, path: &str, mode: Value) -> KResult<()> {
        if self.phase != Phase::Main {
            return Err(
                eyre::eyre!("File system actions are only possible in the 'main' phase").into(),
            );
        }
        self.verify_path(path)?;
        let numeric_mode = match mode {
            Value::Integer(m) => Mode::new(m as u32),
            Value::String(str) => {
                let guard = str.borrow_ref().context("Borrow guard failed")?;
                // Convert text mode (u+rx,g+rw,o+r, etc) to numeric mode
                Mode::parse(&guard)?
            }
            _ => return Err(eyre::eyre!("Invalid mode value").into()),
        };

        self.fs_actions.push(FsInstruction {
            op: FsOp::SetMode { mode: numeric_mode },
            path: path.into(),
            comment: None,
            pkg: None,
        });
        Ok(())
    }

    /// Set all permissions at once
    #[rune::function(keep)]
    pub fn perms(&mut self, path: &str, owner: &str, group: &str, mode: Value) -> KResult<()> {
        self.chown(path, owner)?;
        self.chgrp(path, group)?;
        self.chmod(path, mode)?;
        Ok(())
    }
}

#[rune::module(::command)]
/// Commands describe the changes to apply to the system
pub(crate) fn module() -> Result<Module, ContextError> {
    let mut m = Module::from_meta(self::module_meta)?;
    m.ty::<Commands>()?;
    m.function_meta(Commands::ignore_path__meta)?;
    m.function_meta(Commands::add_pkg__meta)?;
    m.function_meta(Commands::remove_pkg__meta)?;
    m.function_meta(Commands::rm__meta)?;
    m.function_meta(Commands::has_source_file__meta)?;
    m.function_meta(Commands::copy__meta)?;
    m.function_meta(Commands::copy_from__meta)?;
    m.function_meta(Commands::ln__meta)?;
    m.function_meta(Commands::write__meta)?;
    m.function_meta(Commands::mkdir__meta)?;

    m.function_meta(Commands::chown__meta)?;
    m.function_meta(Commands::chgrp__meta)?;
    m.function_meta(Commands::chmod__meta)?;
    m.function_meta(Commands::perms__meta)?;

    Ok(m)
}
