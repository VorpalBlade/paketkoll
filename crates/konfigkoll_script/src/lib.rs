//! Scripting language interface for konfigkoll.
//!
//! This provides the glue between Rust and Rune, in particular the custom
//! Rune modules that konfigkoll provides.
//!
//! This is an internal crate with no stability guarantees whatsoever on the
//! Rust side. The Rune API is also currently heavily unstable but is expected
//! to be stabilized in the future.
//!
//! You should use [`konfigkoll`](https://crates.io/crates/konfigkoll) the
//! command line tool instead.

pub use engine::EngineState;
pub use engine::ScriptEngine;
pub use plugins::command::Commands;
pub use plugins::settings::Settings;
pub use types::Phase;

mod engine;
mod plugins;
mod types;
