//! Exact MIR contract and pass pipeline for the program rewrite.
mod analysis;
mod lower;
mod model;
mod transforms;
mod validate;

pub(crate) use analysis::analyze_program;
pub use lower::lower_to_canonical;
pub use model::{
    Body, Callable, CallableId, CallableKind, LocalDecl, MatchArm, MatchPattern, Op, Program,
    Region, Terminator, ValueOp,
};
pub(crate) use transforms::{canonicalize_program, inline_functions};
pub(crate) use validate::verify_program;

#[cfg(test)]
mod tests;
