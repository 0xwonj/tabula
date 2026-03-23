mod mode;
mod surface;

pub(crate) use mode::validate_free_execution_requirements;
pub(crate) use surface::validate_execution_state_surface;
#[cfg(feature = "prove")]
pub(crate) use surface::{validate_proof_state_surface, validate_prove_input_prestate};
