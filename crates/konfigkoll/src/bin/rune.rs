//! This is a helper binary for konfigkoll that provides Rune support functions
//! such as:
//!
//! * Documentation generation
//! * LSP langauge server
//! * Formatting of rune files
//! * Syntax checking
use konfigkoll_script::ScriptEngine;

#[cfg(target_env = "musl")]
use mimalloc::MiMalloc;

#[cfg(target_env = "musl")]
#[cfg_attr(target_env = "musl", global_allocator)]
static GLOBAL: MiMalloc = MiMalloc;

fn main() {
    rune::cli::Entry::new()
        .about(format_args!("konfigkoll rune cli"))
        .context(&mut |_opts| ScriptEngine::create_context())
        .run();
}
