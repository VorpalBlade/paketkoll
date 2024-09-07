use crate::plugins::command::Commands;
use crate::plugins::error::KError;
use crate::plugins::package_managers::PackageManagers;
use crate::plugins::properties::Properties;
use crate::plugins::settings::Settings;
use camino::Utf8Path;
use camino::Utf8PathBuf;
use color_eyre::Section;
use color_eyre::SectionExt;
use eyre::WrapErr;
use paketkoll_types::backend::Backend;
use paketkoll_types::backend::Files;
use paketkoll_types::backend::PackageBackendMap;
use paketkoll_types::backend::PackageMapMap;
use paketkoll_types::intern::Interner;
use rune::termcolor::Buffer;
use rune::termcolor::ColorChoice;
use rune::termcolor::StandardStream;
use rune::Diagnostics;
use rune::Source;
use rune::Vm;
use std::fmt::Display;
use std::panic::catch_unwind;
use std::panic::AssertUnwindSafe;
use std::sync::Arc;
use std::sync::OnceLock;

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
    pub fn as_str(self) -> &'static str {
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

/// State being built up by the scripts as it runs
#[derive(Debug)]
pub struct EngineState {
    /// Properties set by the user
    pub(crate) properties: Properties,
    /// Commands to be applied to the system
    pub(crate) commands: Commands,
    /// Settings of how konfigkoll should behave.
    pub(crate) settings: Arc<Settings>,
    /// All the enabled package managers
    pub(crate) package_managers: Option<PackageManagers>,
}

/// Path to the configuration directory
pub(crate) static CFG_PATH: OnceLock<Utf8PathBuf> = OnceLock::new();

impl EngineState {
    pub fn new(files_path: Utf8PathBuf) -> Self {
        let settings = Arc::new(Settings::default());
        Self {
            properties: Default::default(),
            commands: Commands::new(files_path, settings.clone()),
            settings,
            package_managers: None,
        }
    }

    pub fn setup_package_managers(
        &mut self,
        package_backends: &PackageBackendMap,
        file_backend_id: Backend,
        files_backend: &Arc<dyn Files>,
        package_maps: &PackageMapMap,
        interner: &Arc<Interner>,
    ) {
        self.package_managers = Some(PackageManagers::create_from(
            package_backends,
            file_backend_id,
            files_backend,
            package_maps,
            interner,
        ));
    }

    pub fn settings(&self) -> Arc<Settings> {
        Arc::clone(&self.settings)
    }

    pub fn commands(&self) -> &Commands {
        &self.commands
    }

    pub fn commands_mut(&mut self) -> &mut Commands {
        &mut self.commands
    }
}

/// The script engine that is the main entry point for this crate.
#[derive(Debug)]
pub struct ScriptEngine {
    runtime: Arc<rune::runtime::RuntimeContext>,
    sources: rune::Sources,
    /// User scripts
    unit: Arc<rune::Unit>,
    /// Properties exposed by us or set by the user
    pub(crate) state: EngineState,
}

impl ScriptEngine {
    pub fn create_context() -> Result<rune::Context, rune::ContextError> {
        let mut context = rune::Context::with_default_modules()?;

        // Register modules
        crate::plugins::register_modules(&mut context)?;
        context.install(rune_modules::json::module(true)?)?;
        context.install(rune_modules::toml::module(true)?)?;
        context.install(rune_modules::toml::de::module(true)?)?;
        context.install(rune_modules::toml::ser::module(true)?)?;

        Ok(context)
    }

    pub fn new_with_files(config_path: &Utf8Path) -> eyre::Result<Self> {
        CFG_PATH.set(config_path.to_owned()).map_err(|v| {
            eyre::eyre!("Failed to set CFG_PATH to {v}, this should not be called more than once")
        })?;
        let context = Self::create_context()?;

        // Create state
        let state = EngineState::new(config_path.join("files"));

        // Load scripts
        let mut diagnostics = Diagnostics::new();

        let mut sources = rune::Sources::new();
        sources
            .insert(
                Source::from_path(config_path.join("main.rn"))
                    .wrap_err("Failed to load main.rn")?,
            )
            .wrap_err("Failed to insert source file")?;

        let result = rune::prepare(&mut sources)
            .with_context(&context)
            .with_diagnostics(&mut diagnostics)
            .build();

        if !diagnostics.is_empty() {
            let mut writer = StandardStream::stderr(ColorChoice::Always);
            diagnostics.emit(&mut writer, &sources)?;
        }

        // Create ScriptEngine
        Ok(Self {
            runtime: Arc::new(context.runtime()?),
            sources,
            state,
            unit: Arc::new(result?),
        })
    }

    /// Call a function in the script
    #[tracing::instrument(level = "info", name = "script", skip(self))]
    pub async fn run_phase(&mut self, phase: Phase) -> eyre::Result<()> {
        // Update phase in relevant state
        self.state.commands.phase = phase;
        // Create VM and do call
        let mut vm = Vm::new(self.runtime.clone(), self.unit.clone());
        tracing::info!("Calling script");
        let output = match phase {
            Phase::SystemDiscovery => {
                vm.async_call(
                    [phase.as_str()],
                    (&mut self.state.properties, self.state.settings.as_ref()),
                )
                .await
            }
            Phase::Ignores | Phase::ScriptDependencies => {
                vm.async_call(
                    [phase.as_str()],
                    (&mut self.state.properties, &mut self.state.commands),
                )
                .await
            }
            Phase::Main => {
                vm.async_call(
                    [phase.as_str()],
                    (
                        &mut self.state.properties,
                        &mut self.state.commands,
                        self.state
                            .package_managers
                            .as_ref()
                            .expect("Package managers must be set"),
                    ),
                )
                .await
            }
        };
        // Handle rune runtime errors
        let output = match output {
            Ok(output) => output,
            Err(e) => {
                let err_str = format!("Rune error while executing {phase}: {}", &e);
                tracing::error!("{}", err_str);
                let mut writer = Buffer::ansi();
                e.emit(&mut writer, &self.sources)?;

                let rune_diag =
                    std::str::from_utf8(writer.as_slice().trim_ascii_end())?.to_string();

                return Err(e)
                    .context("Rune runtime error")
                    .section(rune_diag.header(
                        "  ━━━━━━━━━━━━━━━━━━━━━━━━ Rune Diagnostics and Backtrace \
                         ━━━━━━━━━━━━━━━━━━━━━━━━\n",
                    ));
            }
        };
        tracing::info!("Returned from script");
        // Do error handling on the returned result
        match output {
            rune::Value::Result(result) => match result.borrow_ref()?.as_ref() {
                Ok(_) => (),
                Err(e) => vm.with(|| try_format_error(phase, e))?,
            },
            _ => eyre::bail!("Got non-result from {phase}: {output:?}"),
        }
        Ok(())
    }

    #[inline]
    pub fn state(&self) -> &EngineState {
        &self.state
    }

    #[inline]
    pub fn state_mut(&mut self) -> &mut EngineState {
        &mut self.state
    }
}

/// Attempt to format the error in the best way possible.
///
/// Unfortunately this is awkward with dynamic Rune values.
fn try_format_error(phase: Phase, value: &rune::Value) -> eyre::Result<()> {
    match value.clone().into_any() {
        rune::runtime::VmResult::Ok(any) => {
            if let Ok(mut err) = any.downcast_borrow_mut::<KError>() {
                tracing::error!("Got error result from {phase}: {}", *err.inner());
                let err: eyre::Error = err.take_inner();
                return Err(err);
            }
            if let Ok(err) = any.downcast_borrow_ref::<std::io::Error>() {
                eyre::bail!("Got IO error result from {phase}: {:?}", *err);
            }
            let ty = try_get_type_info(value, "error");
            let formatted = catch_unwind(AssertUnwindSafe(|| format!("{value:?}")));
            eyre::bail!(
                "Got error result from {phase}, but it is a unknown error type: {ty}: {any:?}, \
                 formats as: {formatted:?}",
            );
        }
        rune::runtime::VmResult::Err(not_any) => {
            tracing::error!(
                "Got error result from {phase}, it was not an Any: {not_any:?}. Trying other \
                 approaches at printing the error."
            );
        }
    }
    // Attempt to format the error
    let formatted = catch_unwind(AssertUnwindSafe(|| {
        format!("Got error result from {phase}: {value:?}")
    }));
    match formatted {
        Ok(str) => eyre::bail!(str),
        Err(_) => {
            let ty = try_get_type_info(value, "error");
            eyre::bail!(
                "Got error result from {phase}, but got a panic while attempting to format said \
                 error for printing, {ty}",
            );
        }
    }
}

/// Best effort attempt at gettint the type info and printing it
fn try_get_type_info(e: &rune::Value, what: &str) -> String {
    match e.type_info() {
        rune::runtime::VmResult::Ok(ty) => format!("type info for {what}: {ty:?}"),
        rune::runtime::VmResult::Err(err) => {
            format!("failed getting type info for {what}: {err:?}")
        }
    }
}
