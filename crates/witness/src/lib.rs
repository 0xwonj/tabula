//! Witness pipeline and trace builder for the Tabula proof system.
//!
//! Transforms executor output and runtime-owned proof inputs into canonical
//! chip traces (`TraceMap`) for STARK proving.
//!
//! Public surface policy:
//! - root exports only the minimal shared preparation seam
//! - builtin lowering helpers stay namespaced under [`trace::builtin`]
//! - runtime-owned column transition assembly lives outside this crate

pub mod prepare;
/// Program-level proof-optimization metadata types.
pub mod program_info {
    pub use crate::witness::program_info::{LiteralCell, ProgramInfo, TemplateId};
}
pub mod trace;
mod witness;

pub use prepare::{BatchInputPreparer, PreparedExecutionInputs};
pub use witness::{AccessRow, InitRow};
