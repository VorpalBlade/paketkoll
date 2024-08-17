//! Exposing regex to rune

use super::error::KResult;
use eyre::WrapErr;
use regex::Regex as InnerRegex;
use rune::Any;
use rune::ContextError;
use rune::Module;

#[derive(Debug, Clone, Any)]
#[rune(item = ::regex)]
/// A UTF-8 string regular expression
pub struct Regex {
    inner: InnerRegex,
}

/// Rust API
impl Regex {
    pub fn inner(&self) -> &InnerRegex {
        &self.inner
    }
}

/// Rune API
impl Regex {
    /// Create a new regex from a string
    #[rune::function(path = Self::new)]
    fn new(pattern: &str) -> KResult<Self> {
        Ok(Self {
            inner: InnerRegex::new(pattern).wrap_err("Failed to compile regular expression")?,
        })
    }

    /// Check if the regex matches the string
    #[rune::function]
    fn is_match(&self, text: &str) -> bool {
        self.inner.is_match(text)
    }

    /// Find the first match in the string
    #[rune::function]
    fn find(&self, text: &str) -> Option<(usize, usize)> {
        self.inner.find(text).map(|m| (m.start(), m.end()))
    }

    /// Replace the leftmost match in the string.
    ///
    /// Capture groups can be referred to via `$1`, `$2`, etc. (`$0` is the full
    /// match). Named capture groups are supported via `$name`.
    /// You can also use `${name}` or `${1}` etc., which is often needed to
    /// disambiguate when a capture group number is followed by literal
    /// text.
    #[rune::function]
    fn replace(&self, text: &str, replace: &str) -> String {
        self.inner.replace(text, replace).to_string()
    }

    /// Replace all matches in the string
    ///
    /// Capture groups can be referred to via `$1`, `$2`, etc. (`$0` is the full
    /// match). Named capture groups are supported via `$name`.
    #[rune::function]
    fn replace_all(&self, text: &str, replace: &str) -> String {
        self.inner.replace_all(text, replace).to_string()
    }

    /// Capture groups
    ///
    /// * If no match is found returns `None`.
    /// * Otherwise Some(vector of optional strings) where:
    ///     * The first group (index 0) is the full match as `Some(value)`.
    ///     * The rest are the capture groups. If they didn't match they are
    ///       `None`. Otherwise, they are `Some(value)`.
    #[rune::function]
    fn captures(&self, text: &str) -> Option<Vec<Option<String>>> {
        let captures = self.inner.captures(text)?;
        Some(
            captures
                .iter()
                .map(|m| m.map(|v| v.as_str().to_string()))
                .collect(),
        )
    }
}

#[rune::module(::regex)]
/// A wrapper for the rust regex crate
pub(crate) fn module() -> Result<Module, ContextError> {
    let mut m = Module::from_meta(self::module_meta)?;
    m.ty::<Regex>()?;
    m.function_meta(Regex::new)?;
    m.function_meta(Regex::is_match)?;
    m.function_meta(Regex::find)?;
    m.function_meta(Regex::replace)?;
    m.function_meta(Regex::replace_all)?;
    m.function_meta(Regex::captures)?;
    Ok(m)
}
