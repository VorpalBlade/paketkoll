//! Helpers for working with systemd units

use std::str::FromStr;
use std::sync::LazyLock;

use camino::Utf8PathBuf;
use compact_str::CompactString;
use eyre::Context;
use rune::Any;
use rune::ContextError;
use rune::Module;

use super::error::KResult;
use super::package_managers::OriginalFilesError;
use super::package_managers::PackageManager;
use crate::Commands;
use crate::Phase;

/// A systemd Unit file
///
/// This can be used to enable or mask systemd units.
///
/// For example, to enable a unit file from a package:
///
/// ```rune
/// systemd::Unit::from_pkg("util-linux",
///                          "fstrim.timer",
///                          package_managers.files())
///     .enable(ctx.cmds)?;
/// ```
///
/// The additional functions can be used to customise the behaviour.
#[derive(Debug, Any)]
#[rune(item = ::systemd)]
struct Unit {
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

impl Unit {
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

    /// Where we expect the file to be (for the purpose of symlink target and
    /// finding the file contents)
    fn unit_file_path(&self) -> String {
        let base_path = match self.type_ {
            Type::System => BASE_PATHS.0.as_path(),
            Type::User => BASE_PATHS.1.as_path(),
        };
        match &self.source {
            Source::File { path, .. } => path.to_string(),
            Source::Package { .. } => {
                format!("{}/{}", base_path, self.unit)
            }
        }
    }

    /// Get contents of file
    fn contents(&self) -> eyre::Result<Vec<u8>> {
        match &self.source {
            Source::File { contents, .. } => Ok(contents.clone()),
            Source::Package {
                package_manager,
                package,
            } => {
                let path = &self.unit_file_path();
                match package_manager.file_contents(package, path) {
                    Ok(v) => Ok(v),
                    Err(OriginalFilesError::FileNotFound(_, _)) => {
                        // Try again with/without /usr, because Debian hasn't finished the /usr
                        // merge. Still.
                        let alt_path = if path.starts_with("/usr") {
                            self.unit_file_path().replacen("/usr", "", 1)
                        } else {
                            format!("/usr{}", self.unit_file_path())
                        };
                        Ok(package_manager
                            .file_contents(package, &alt_path)
                            .context("File contents query failed")?)
                    }
                    Err(e) => Err(e).context("File contents query failed")?,
                }
            }
        }
    }

    /// Parse the contents of the unit file, it is a simple INI file, use
    /// rust-ini
    fn parse_unit_file(&self) -> eyre::Result<ini::Ini> {
        let contents = self.contents()?;
        let contents = std::str::from_utf8(&contents)
            .context("UTF-8 conversion failed for systemd unit file")?;
        ini::Ini::load_from_str(contents).context("Parsing unit file as INI failed")
    }
}

/// Rune API
impl Unit {
    /// Create a new instance from a file path
    #[rune::function(path = Self::from_file, keep)]
    pub fn from_file(file: &str, cmds: &Commands) -> KResult<Self> {
        Ok(Self {
            unit: file
                .rsplit_once('/')
                .map(|(_, f)| f)
                .ok_or_else(|| eyre::eyre!("No file name found"))?
                .into(),
            source: Source::File {
                path: file.into(),
                contents: cmds
                    .file_contents(file)
                    .ok_or_else(|| {
                        eyre::eyre!(
                            "Failed to find file contents of {} (did you add a command that \
                             created the file before?)",
                            file
                        )
                    })?
                    .contents()?
                    .into_owned(),
            },
            type_: Type::System,
            name: None,
            process_aliases: true,
            process_wanted_by: true,
        })
    }

    /// Create a new instace from a unit file in a package
    #[rune::function(path = Self::from_pkg, keep)]
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
    #[rune::function(keep)]
    pub fn user(mut self) -> Self {
        self.type_ = Type::User;
        self
    }

    /// Override the name of the unit. Useful for parameterised units (e.g.
    /// `foo@.service`)
    #[rune::function(keep)]
    pub fn name(mut self, name: &str) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Skip installing aliases
    #[rune::function(keep)]
    pub fn skip_aliases(mut self) -> Self {
        self.process_aliases = false;
        self
    }

    /// Skip installing wanted-by
    #[rune::function(keep)]
    pub fn skip_wanted_by(mut self) -> Self {
        self.process_wanted_by = false;
        self
    }

    /// Enable the unit
    #[rune::function(keep)]
    pub fn enable(self, commands: &mut Commands) -> KResult<()> {
        if commands.phase != Phase::Main {
            return Err(
                eyre::eyre!("File system actions are only possible in the 'main' phase").into(),
            );
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
    #[rune::function(keep)]
    pub fn mask(self, commands: &mut Commands) -> KResult<()> {
        if commands.phase != Phase::Main {
            return Err(
                eyre::eyre!("File system actions are only possible in the 'main' phase").into(),
            );
        }

        commands.ln(&self.symlink_path(), "/dev/null")?;
        Ok(())
    }
}

static BASE_PATHS: LazyLock<(Utf8PathBuf, Utf8PathBuf)> = LazyLock::new(|| {
    let mut cmd = std::process::Command::new("systemd-path");
    cmd.args(["systemd-system-unit", "systemd-user-unit"]);
    let output = cmd.output().expect("Failed to run systemd-path");
    let mut paths = output.stdout.split(|&b| b == b'\n').map(|b| {
        Utf8PathBuf::from_str(std::str::from_utf8(b).expect("Failed to parse as UTF-8"))
            .expect("Ill-formed path")
    });
    (
        paths.next().expect("Not even one line"),
        paths.next().expect("Two lines"),
    )
});

#[rune::module(::systemd)]
/// Functionality to simplify working with systemd
pub(crate) fn module() -> Result<Module, ContextError> {
    let mut m = Module::from_meta(self::module_meta)?;
    m.ty::<Unit>()?;
    m.function_meta(Unit::from_file__meta)?;
    m.function_meta(Unit::from_pkg__meta)?;
    m.function_meta(Unit::user__meta)?;
    m.function_meta(Unit::name__meta)?;
    m.function_meta(Unit::skip_aliases__meta)?;
    m.function_meta(Unit::skip_wanted_by__meta)?;
    m.function_meta(Unit::enable__meta)?;
    m.function_meta(Unit::mask__meta)?;
    Ok(m)
}
