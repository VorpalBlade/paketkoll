//! RHAI plugins for Konfigkoll

pub(crate) mod command;
mod host_fs;
pub mod package_managers;
mod passwd;
mod patch;
pub(crate) mod properties;
mod regex;
pub(crate) mod settings;
mod shell;
mod sysinfo;
mod systemd;

pub(crate) fn register_modules(context: &mut rune::Context) -> Result<(), rune::ContextError> {
    context.install(command::module()?)?;
    context.install(host_fs::module()?)?;
    context.install(package_managers::module()?)?;
    context.install(properties::module()?)?;
    context.install(regex::module()?)?;
    context.install(settings::module()?)?;
    context.install(sysinfo::module()?)?;
    context.install(systemd::module()?)?;

    Ok(())
}
