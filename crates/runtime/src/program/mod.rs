mod binding;
#[cfg(feature = "prove")]
mod contract;

pub use binding::Binding;
pub(crate) use binding::binding_from_artifact;
#[cfg(feature = "prove")]
pub(crate) use binding::binding_from_compiled_program;
#[cfg(feature = "prove")]
pub use contract::{
    ColumnProofSlot, PrecompileProofSlot, ProofPlan, ResolvedProofProgram, RuntimeProgram,
};
