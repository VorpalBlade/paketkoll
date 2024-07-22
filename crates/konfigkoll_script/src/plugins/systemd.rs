//! Helpers for working with systemd units

use camino::Utf8PathBuf;
use compact_str::CompactString;
use rune::{Any, ContextError, Module};

use crate::{Commands, Phase};

use super::package_managers::PackageManager;

#[derive(Debug, Any)]
#[rune(item = ::systemd)]
struct Systemd {
    unit: CompactString,
    source: Source,
    type_: Type,
    name: Option<CompactString>,
    process_aliases: bool,
    process_wanted_by: bool,
}

#[derive(Debug)]
enum Type {
    System,
    User,
}

impl Type {
    fn as_str(&self) -> &'static str {
        match self {
            Self::System => "system",
            Self::User => "user",
        }
    }
}

#[derive(Debug, Clone)]
enum Source {
    File {
        path: Utf8PathBuf,
        contents: Vec<u8>,
    },
    Package {
        package_manager: PackageManager,
        package: CompactString,
    },
}

impl Systemd {
    fn symlink_name(&self) -> &str {
        match &self.name {
            Some(name) => name.as_str(),
            None => &self.unit,
        }
    }

    /// Where we expect the symlink to be created
    fn symlink_path(&self) -> String {
        format!(
            "/etc/systemd/{}/{}",
            self.type_.as_str(),
            self.symlink_name()
        )
    }

    /// Where we expect the file to be (for the purpose of symlink target and finding the file contents)
    fn unit_file_path(&self) -> String {
        match &self.source {
            Source::File { path, .. } => path.to_string(),
            Source::Package { .. } => {
                format!("/usr/lib/systemd/{}/{}", self.type_.as_str(), self.unit)
            }
        }
    }

    /// Get contents of file
    fn contents(&self) -> anyhow::Result<Vec<u8>> {
        match &self.source {
            Source::File { contents, .. } => Ok(contents.clone()),
            Source::Package {
                package_manager,
                package,
            } => package_manager.file_contents(package, &self.unit_file_path()),
        }
    }

    /// Parse the contents of the unit file, it is a simple INI file, use rust-ini
    fn parse_unit_file(&self) -> anyhow::Result<ini::Ini> {
        let contents = self.contents()?;
        let contents = std::str::from_utf8(&contents)?;
        Ok(ini::Ini::load_from_str(contents)?)
    }
}

/// Rune API
impl Systemd {
    /// Create a new instance from a file path
    #[rune::function(path = Self::from_file)]
    pub fn from_file(file: &str, cmds: &Commands) -> anyhow::Result<Self> {
        Ok(Self {
            unit: file.rsplit_once('/').map(|(_, f)| f).ok_or_else(|| anyhow::anyhow!("No file name found"))?.into(),
            source: Source::File {
                path: file.into(),
                contents: cmds.file_contents(file).ok_or_else(|| {
                    anyhow::anyhow!("Failed to find file contents of {} (did you add a command that created the file before?)", file)
                })?.contents()?.into_owned(),
            },
            type_: Type::System,
            name: None,
            process_aliases: true,
            process_wanted_by: true,
        })
    }

    /// Create a new instace from a unit file in a package
    #[rune::function(path = Self::from_pkg)]
    pub fn from_pkg(package: &str, unit: &str, package_manager: &PackageManager) -> Self {
        Self {
            unit: unit.into(),
            source: Source::Package {
                package_manager: package_manager.clone(),
                package: package.into(),
            },
            type_: Type::System,
            name: None,
            process_aliases: true,
            process_wanted_by: true,
        }
    }

    /// Mark this as a user unit instead of (the default) system unit type
    #[rune::function]
    pub fn user(mut self) -> Self {
        self.type_ = Type::User;
        self
    }

    /// Override the name of the unit. Useful for parameterised units (e.g. `foo@.service`)
    #[rune::function]
    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Skip installing aliases
    #[rune::function]
    pub fn skip_aliases(mut self) -> Self {
        self.process_aliases = false;
        self
    }

    /// Skip installing wanted-by
    #[rune::function]
    pub fn skip_wanted_by(mut self) -> Self {
        self.process_wanted_by = false;
        self
    }

    /// Enable the unit
    #[rune::function]
    pub fn enable(self, commands: &mut Commands) -> anyhow::Result<()> {
        if commands.phase != Phase::Main {
            return Err(anyhow::anyhow!(
                "File system actions are only possible in the 'main' phase"
            ));
        }

        let parsed = self.parse_unit_file()?;
        let install_section = parsed.section(Some("Install"));

        let type_ = self.type_.as_str();
        let name = self.symlink_name();
        let unit_path = self.unit_file_path();

        if let Some(install_section) = install_section {
            if self.process_aliases {
                for alias in install_section.get_all("Alias") {
                    for alias in alias.split_ascii_whitespace() {
                        let p = format!("/etc/systemd/{}/{}", type_, alias);
                        commands.ln(&p, &unit_path)?;
                    }
                }
            }

            if self.process_wanted_by {
                for wanted_by in install_section.get_all("WantedBy") {
                    for wanted_by in wanted_by.split_ascii_whitespace() {
                        let p = format!("/etc/systemd/{}/{}.wants/{}", type_, wanted_by, name);
                        commands.ln(&p, &unit_path)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Mask the unit
    #[rune::function]
    pub fn mask(self, commands: &mut Commands) -> anyhow::Result<()> {
        if commands.phase != Phase::Main {
            return Err(anyhow::anyhow!(
                "File system actions are only possible in the 'main' phase"
            ));
        }

        commands.ln(&self.symlink_path(), "/dev/null")?;
        Ok(())
    }
}

#[rune::module(::systemd)]
/// Various functions to get system information
pub(crate) fn module() -> Result<Module, ContextError> {
    let mut m = Module::from_meta(self::module_meta)?;
    m.ty::<Systemd>()?;
    m.function_meta(Systemd::from_file)?;
    m.function_meta(Systemd::from_pkg)?;
    m.function_meta(Systemd::user)?;
    m.function_meta(Systemd::name)?;
    m.function_meta(Systemd::skip_aliases)?;
    m.function_meta(Systemd::skip_wanted_by)?;
    m.function_meta(Systemd::enable)?;
    m.function_meta(Systemd::mask)?;
    Ok(m)
}
