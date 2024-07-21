//! Functions for properties
use ahash::AHashMap;
use compact_str::CompactString;
use itertools::Itertools;
use rune::ContextError;
use rune::Module;

pub type PropertyKey = CompactString;
pub type PropertyValue = rune::runtime::Value;

/// Configuration and persistent (across phases) properties.
///
/// It is recommended to store properties using structrured system, such as
/// creating a hierarchy separated by `.`. But it is up to you.
#[derive(Debug, Default, rune::Any)]
#[rune(item = ::properties)]
pub(crate) struct Properties {
    properties: AHashMap<PropertyKey, PropertyValue>,
}

impl Properties {
    /// Get a user defined property
    ///
    /// Will return `()` if the property does not exist.
    #[rune::function]
    pub fn get(&self, name: &str) -> Option<PropertyValue> {
        self.properties.get(name).cloned()
    }

    /// Set a user defined property
    #[rune::function]
    pub fn set(&mut self, name: &str, value: PropertyValue) {
        self.properties.insert(name.into(), value);
    }

    /// Check if a property exists
    #[rune::function]
    pub fn has(&self, name: &str) -> bool {
        self.properties.contains_key(name)
    }

    /// Dump all properties to the terminal. For debugging
    #[rune::function]
    pub fn dump(&self) {
        for (key, value) in self.properties.iter().sorted_by(|a, b| a.0.cmp(b.0)) {
            println!("{} = {:?}", key, value);
        }
    }
}

#[rune::module(::properties)]
/// User defined persistent (between phases) properties
pub(crate) fn module() -> Result<Module, ContextError> {
    let mut m = Module::from_meta(self::module_meta)?;
    m.ty::<Properties>()?;
    m.function_meta(Properties::get)?;
    m.function_meta(Properties::set)?;
    m.function_meta(Properties::has)?;
    m.function_meta(Properties::dump)?;
    Ok(m)
}
