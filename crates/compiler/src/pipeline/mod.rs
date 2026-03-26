//! Canonical source compiler surface for the rewritten language pipeline.

mod compile;
mod diagnostics;
mod fingerprint;
mod io;
mod types;

pub use crate::registration::{compile_and_register_program_source, register_compiled_program};
pub use compile::{compile_program_source, compile_program_source_with_catalogs};
pub use io::{load_registered_program, parse_registered_program};
pub use types::{CompiledProgram, RegisteredProgram, StateFieldSchemeBinding};

#[cfg(test)]
mod tests;
