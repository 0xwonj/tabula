use tabula_compiler::SealedProgram;

use crate::error::RuntimeError;

pub(crate) fn validate_free_execution_requirements(
    compiled_program: &SealedProgram,
) -> Result<(), RuntimeError> {
    if !compiled_program.precompile_manifest().is_empty() {
        return Err(RuntimeError::ValidationFailed {
            detail:
                "program requires precompiles; build a TabulaRuntime with a HostEnvironment that installs the required precompile backends before execution"
                    .to_string(),
        });
    }
    if !compiled_program.required_property_requirements().is_empty() {
        return Err(RuntimeError::ValidationFailed {
            detail:
                "program requires scheme-backed property queries; build a TabulaRuntime with a HostEnvironment that installs any required scheme backends before execution"
                    .to_string(),
        });
    }
    Ok(())
}
