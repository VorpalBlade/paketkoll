use std::fmt::Display;

/// Describe the phases of script evaluation.
///
/// Each phase is a separate function defined by the top level script.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Phase {
    /// During this phase, the script can discover information about the system
    /// and hardware, and set properties for later use.
    #[default]
    SystemDiscovery,
    /// During this phase file system ignores should be set up. These are
    /// needed by the file system scan code that will be started concurrently
    /// after this.
    Ignores,
    /// Early package dependencies that are needed by the main phase should be
    /// declared here. These packages will be installed before the main config
    /// runs if they are missing.
    ScriptDependencies,
    /// During the main phase the config proper is generated.
    Main,
}

impl Phase {
    /// Convert to string
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SystemDiscovery => "phase_system_discovery",
            Self::Ignores => "phase_ignores",
            Self::ScriptDependencies => "phase_script_dependencies",
            Self::Main => "phase_main",
        }
    }
}

impl Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}
