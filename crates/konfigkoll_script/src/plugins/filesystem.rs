//! Host file system access

use std::io::{ErrorKind, Read};

use anyhow::Context;
use camino::Utf8PathBuf;
use rune::alloc::fmt::TryWrite;
use rune::{
    runtime::{Bytes, Formatter},
    vm_write, Any, ContextError, Module,
};

use konfigkoll_core::utils::safe_path_join;

use crate::engine::CFG_PATH;

/// A file error
#[derive(Debug, Any, thiserror::Error)]
#[rune(item = ::filesystem)]
enum FileError {
    #[error("IO Error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Allocation error: {0}")]
    AllocError(#[from] rune::alloc::Error),
}

impl FileError {
    #[rune::function(vm_result, protocol = STRING_DISPLAY)]
    pub(crate) fn display(&self, f: &mut Formatter) {
        vm_write!(f, "{}", self);
    }

    #[rune::function(vm_result, protocol = STRING_DEBUG)]
    pub(crate) fn debug(&self, f: &mut Formatter) {
        vm_write!(f, "{:?}", self);
    }
}

/// Represents a temporary directory
///
/// The directory will be removed when this object is dropped
#[derive(Debug, Any)]
#[rune(item = ::filesystem)]
struct TempDir {
    path: Utf8PathBuf,
}

impl Drop for TempDir {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.path).expect("Failed to remove temporary directory");
    }
}

impl TempDir {
    #[rune::function(vm_result, protocol = STRING_DEBUG)]
    fn debug(&self, f: &mut Formatter) {
        vm_write!(f, "{:?}", self);
    }

    /// Create a new temporary directory
    #[rune::function(path = Self::new)]
    fn new() -> anyhow::Result<Self> {
        let dir = tempfile::TempDir::with_prefix("konfigkoll_")?.into_path();
        match Utf8PathBuf::from_path_buf(dir) {
            Ok(path) => Ok(Self { path }),
            Err(path) => {
                std::fs::remove_dir_all(&path).expect("Failed to remove temporary directory");
                Err(anyhow::anyhow!("Failed to convert path to utf8: {path:?}"))
            }
        }
    }

    /// Get the path to the temporary directory
    #[rune::function]
    fn path(&self) -> String {
        self.path.to_string()
    }

    /// Write a temporary file under this directory, getting it's path path
    #[rune::function]
    fn write(&self, path: &str, contents: &[u8]) -> anyhow::Result<String> {
        let p = safe_path_join(&self.path, path.into());
        std::fs::write(&p, contents).with_context(|| format!("Failed to write to {p}"))?;
        Ok(p.into_string())
    }

    /// Read a file from the temporary directory
    ///
    /// Returns a `Result<Bytes>`
    #[rune::function]
    fn read(&self, path: &str) -> anyhow::Result<Bytes> {
        let p = safe_path_join(&self.path, path.into());
        let data = std::fs::read(&p).with_context(|| format!("Failed to read {p}"))?;
        Ok(Bytes::from_vec(data.try_into()?))
    }
}

/// Represents an open file
#[derive(Debug, Any)]
#[rune(item = ::filesystem)]
struct File {
    file: std::fs::File,
    // TODO: Needed for future privilege separation
    #[allow(dead_code)]
    need_root: bool,
}

/// Rune API
impl File {
    #[rune::function(vm_result, protocol = STRING_DEBUG)]
    pub(crate) fn debug(&self, f: &mut Formatter) {
        vm_write!(f, "{:?}", self);
    }

    /// Open a file (with normal user permissions)
    #[rune::function(path = Self::open)]
    pub fn open(path: &str) -> anyhow::Result<Self> {
        let file = std::fs::File::open(path).with_context(|| format!("Failed to open {path}"))?;
        Ok(Self {
            file,
            need_root: false,
        })
    }

    /// Open a file as root
    #[rune::function(path = Self::open_as_root)]
    pub fn open_as_root(path: &str) -> anyhow::Result<Self> {
        let file =
            std::fs::File::open(path).with_context(|| format!("Failed to open {path} as root"))?;
        Ok(Self {
            file,
            need_root: true,
        })
    }

    /// Open a file relative to the config directory.
    ///
    /// This is generally safe (as long as the file exists in the config directory)
    #[rune::function(path = Self::open_from_config)]
    pub fn open_from_config(path: &str) -> anyhow::Result<Self> {
        let p = safe_path_join(CFG_PATH.get().expect("CFG_PATH not set"), path.into());
        let file = std::fs::File::open(&p)
            .with_context(|| format!("Failed to open {path} from config directory, tried {p}"))?;
        Ok(Self {
            file,
            need_root: false,
        })
    }

    /// Read the entire file as a string
    #[rune::function]
    pub fn read_all_string(&mut self) -> Result<String, std::io::Error> {
        let mut buf = String::new();
        self.file.read_to_string(&mut buf)?;
        Ok(buf)
    }

    /// Read the entire file as bytes
    #[rune::function]
    pub fn read_all_bytes(&mut self) -> Result<Bytes, FileError> {
        let mut buf = Vec::new();
        self.file.read_to_end(&mut buf)?;
        let buf = rune::alloc::Vec::try_from(buf)?;
        Ok(buf.into())
    }
}

/// Check if a path exists
///
/// Returns a `Result<bool>`
#[rune::function]
fn exists(path: &str) -> Result<bool, std::io::Error> {
    let metadata = std::fs::symlink_metadata(path);

    match metadata {
        Ok(_) => Ok(true),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

/// Run a glob pattern against the host file system
///
/// Returns a `Result<Vec<String>>`
#[rune::function]
fn glob(pattern: &str) -> anyhow::Result<Vec<String>> {
    let paths = glob::glob(pattern).context("Failed to construct glob")?;

    let mut result = Vec::new();
    for path in paths {
        result.push(path?.to_string_lossy().to_string());
    }

    Ok(result)
}

/// Get the path to the configuration directory
///
/// **Prefer `File::open_from_config` instead if you just want to load data from the config directory**
///
/// This is primarily useful together with the `process` module to pass a
/// path to a file from the configuration directory to an external command.
#[rune::function]
fn config_path() -> String {
    CFG_PATH.get().expect("CFG_PATH not set").to_string()
}

#[rune::module(::filesystem)]
/// Read only access to the host file system and the configuration directory
///
/// # Host file system access
///
/// Be careful with host file system access, since it can make your configuration non-deterministic.
///
/// The main purpose of this is for things that *shouldn't* be stored in your git
/// managed configuration, in particular for passwords and other secrets:
///
/// * Hashed passwords from `/etc/shadow`
/// * Passwords for wireless networks
/// * Passwords for any services needed (such as databases)
///
/// Another use case is to read some system information from `/sys` that isn't
/// already exposed by other APIs
///
/// # Configuration directory access
///
/// This is generally safe, in order to read files that are part of the configuration
/// (if you want to use them as templates for example and fill in some values)
///
/// Use `File::open_from_config` for this. In special circumstances (together with the `process` module)
/// you may also need [`config_path`].
///
/// # Temporary directories
///
/// This is generally not needed when working with konfigkoll, but can be useful
/// for interacting with external commands via the `process` module.
pub(crate) fn module() -> Result<Module, ContextError> {
    let mut m = Module::from_meta(self::module_meta)?;
    m.ty::<File>()?;
    m.function_meta(File::debug)?;
    m.function_meta(File::open)?;
    m.function_meta(File::open_as_root)?;
    m.function_meta(File::open_from_config)?;
    m.function_meta(File::read_all_string)?;
    m.function_meta(File::read_all_bytes)?;
    m.ty::<FileError>()?;
    m.function_meta(FileError::display)?;
    m.function_meta(FileError::debug)?;
    m.ty::<TempDir>()?;
    m.function_meta(TempDir::debug)?;
    m.function_meta(TempDir::new)?;
    m.function_meta(TempDir::path)?;
    m.function_meta(TempDir::read)?;
    m.function_meta(TempDir::write)?;
    m.function_meta(exists)?;
    m.function_meta(glob)?;
    m.function_meta(config_path)?;
    Ok(m)
}
