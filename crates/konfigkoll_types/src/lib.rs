//! Type definitions for konfigkoll backend
//!
//! These are the core operations that the script desugars into and are compared
//! against the system state.
//!
//! This is an internal API crate with no stability guarantees whatsoever.

mod misc;
mod operations;

pub use misc::FileContents;
pub use operations::FsInstruction;
pub use operations::FsOp;
pub use operations::PkgIdent;
pub use operations::PkgInstruction;
pub use operations::PkgInstructions;
pub use operations::PkgOp;
