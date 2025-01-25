//! Support error type

use rune::ContextError;
use rune::Module;
use rune::alloc::fmt::TryWrite;
use rune::runtime::Formatter;
use rune::vm_write;
use std::fmt::Display;

/// Result alias using `KError`
pub type KResult<T, E = KError> = Result<T, E>;

/// An opqaue error type that can be be printed (but does little else)
///
/// This is a wrapper around an internal rich error type and can be handed back
/// to the Rust code to get a detailed backtrace.
#[derive(Debug, rune::Any)]
#[rune(item = ::error)]
pub struct KError {
    inner: Option<eyre::Error>,
}

impl KError {
    pub(crate) fn inner(&self) -> &eyre::Error {
        self.inner.as_ref().expect("Must be initialised")
    }

    /// Get the inner error out.
    ///
    /// Will panic if the inner error has already been taken (as will other
    /// methods on the error)
    pub(crate) fn take_inner(&mut self) -> eyre::Error {
        std::mem::take(&mut self.inner).expect("Must be initialised")
    }
}

impl Display for KError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.inner {
            Some(inner) => Display::fmt(inner, f),
            None => Display::fmt(&eyre::eyre!("<ALREADY TAKEN>"), f),
        }
    }
}

impl From<eyre::Error> for KError {
    fn from(inner: eyre::Error) -> Self {
        Self { inner: Some(inner) }
    }
}

impl From<std::io::Error> for KError {
    fn from(inner: std::io::Error) -> Self {
        Self {
            inner: Some(eyre::Error::from(inner)),
        }
    }
}

impl From<std::fmt::Error> for KError {
    fn from(inner: std::fmt::Error) -> Self {
        Self {
            inner: Some(eyre::Error::from(inner)),
        }
    }
}

impl From<KError> for eyre::Error {
    fn from(error: KError) -> Self {
        error.inner.expect("Must be initialised")
    }
}

impl KError {
    #[rune::function(vm_result, protocol = STRING_DEBUG)]
    fn string_debug(&self, f: &mut Formatter) {
        vm_write!(f, "{:?}", self);
    }

    #[rune::function(vm_result, protocol = STRING_DISPLAY)]
    fn string_display(&self, f: &mut Formatter) {
        vm_write!(f, "{}", self);
    }
}

#[rune::module(::error)]
/// Generic error handling type(s) used by konfigkoll
pub(crate) fn module() -> Result<Module, ContextError> {
    let mut m = Module::from_meta(module_meta)?;
    m.ty::<KError>()?;
    m.function_meta(KError::string_debug)?;
    m.function_meta(KError::string_display)?;

    Ok(m)
}
