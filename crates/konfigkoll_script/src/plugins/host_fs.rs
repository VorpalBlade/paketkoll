//! Host file system access

use std::io::{ErrorKind, Read};

use rune::{runtime::Bytes, Any, ContextError, Module};

#[derive(Debug, Any, thiserror::Error)]
#[rune(item = ::host_fs)]
enum FileError {
    #[error("IO Error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Allocation error: {0}")]
    AllocError(#[from] rune::alloc::Error),
}

#[derive(Debug, Any)]
#[rune(item = ::host_fs)]
struct File {
    file: std::fs::File,
    need_root: bool,
}

/// Rune API
impl File {
    /// Open a file (with normal user permissions)
    #[rune::function(path = Self::open)]
    pub fn open(path: &str) -> Result<Self, std::io::Error> {
        let file = std::fs::File::open(path)?;
        Ok(Self {
            file,
            need_root: false,
        })
    }

    /// Open a file as root
    #[rune::function(path = Self::open_as_root)]
    pub fn open_as_root(path: &str) -> Result<Self, std::io::Error> {
        let file = std::fs::File::open(path)?;
        Ok(Self {
            file,
            need_root: true,
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

#[rune::function]
fn exists(path: &str) -> Result<bool, std::io::Error> {
    let metadata = std::fs::symlink_metadata(path);

    match metadata {
        Ok(_) => Ok(true),
        Err(err) if err.kind() == ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

#[rune::module(::host_fs)]
/// Read only access to the host file system
///
/// Be careful with this, since it can make your configuration non-deterministic.
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
pub(crate) fn module() -> Result<Module, ContextError> {
    let mut m = Module::from_meta(self::module_meta)?;
    m.ty::<File>()?;
    m.function_meta(File::open)?;
    m.function_meta(File::open_as_root)?;
    m.function_meta(File::read_all_string)?;
    m.function_meta(File::read_all_bytes)?;
    m.function_meta(exists)?;
    Ok(m)
}
