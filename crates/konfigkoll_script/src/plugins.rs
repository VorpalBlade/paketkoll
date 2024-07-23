//! RHAI plugins for Konfigkoll

pub(crate) mod command;
mod filesystem;
pub mod package_managers;
mod passwd;
mod patch;
mod process;
pub(crate) mod properties;
pub(crate) mod regex;
pub(crate) mod settings;
mod sysinfo;
mod systemd;

pub(crate) fn register_modules(context: &mut rune::Context) -> Result<(), rune::ContextError> {
    context.install(command::module()?)?;
    context.install(filesystem::module()?)?;
    context.install(package_managers::module()?)?;
    context.install(passwd::module()?)?;
    context.install(patch::module()?)?;
    context.install(process::module(true)?)?;
    context.install(properties::module()?)?;
    context.install(regex::module()?)?;
    context.install(settings::module()?)?;
    context.install(sysinfo::module()?)?;
    context.install(systemd::module()?)?;

    Ok(())
}
