use std::sync::Arc;

use tabula_compiler::RegisteredProgram;
use tabula_contract::ArtifactContext;
use tabula_contract::SealedArtifact;
use tabula_contract::SealedRelationPolicy;
use tabula_core::RootProfileId;
use tabula_ext::backend::ExecutionBackend;
use tabula_ext::backend::ProofColumn;
use tabula_ext::backend::execution::{
    IrHashExecutionBackend, PublicStatementTranscriptExecutionBackend, RelationExecutionBackend,
};
use tabula_ext::root::RootProofBackend;
use tabula_ir as ir;
use tabula_machine::{TabulaMachine, TabulaStarkConfig};
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry};

use crate::bootstrap::machine::{attach_execution_backend, build_machine_builder};
use crate::error::{RuntimeError, SetupError};
use crate::host::SchemeFactoryMap;
use crate::state_runtime::ResolvedStateRuntime;

/// Shared registered-program setup used by both runtime proving and verification.
#[derive(Clone)]
pub(crate) struct ProgramSetup {
    pub(crate) artifact_context: ArtifactContext,
    pub(crate) resolved_state: ResolvedStateRuntime,
    pub(crate) machine_columns: Vec<Arc<dyn ProofColumn>>,
    pub(crate) relation_policy: SealedRelationPolicy,
    pub(crate) uses_ir_hash: bool,
}

/// Resolve the sealed-artifact-facing program setup.
///
/// This is the authoritative implementation; the registered-program variant
/// below is a thin wrapper that delegates here via `registered.sealed()`.
pub(crate) fn resolve_sealed_artifact_setup(
    sealed: &SealedArtifact,
    backend_factories: &SchemeFactoryMap,
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
    accepted_root_binding_families: &[RootProfileId],
) -> Result<ProgramSetup, RuntimeError> {
    let resolved_state = ResolvedStateRuntime::from_sealed_artifact(
        sealed,
        backend_factories,
        type_runtimes,
        encoding_runtimes,
        accepted_root_binding_families,
    )?;
    let machine_columns = resolved_state
        .backends()
        .map(|backend| Arc::clone(&backend.proof_column))
        .collect();
    Ok(ProgramSetup {
        artifact_context: artifact_context_from_sealed(sealed),
        resolved_state,
        machine_columns,
        relation_policy: sealed.relation_policy(),
        uses_ir_hash: sealed.uses_ir_hash(),
    })
}

/// Resolve program setup for a registered program by delegating to the sealed path.
pub(crate) fn resolve_program_setup(
    registered_program: &RegisteredProgram,
    backend_factories: &SchemeFactoryMap,
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
    accepted_root_binding_families: &[RootProfileId],
) -> Result<ProgramSetup, RuntimeError> {
    resolve_sealed_artifact_setup(
        registered_program.sealed(),
        backend_factories,
        type_runtimes,
        encoding_runtimes,
        accepted_root_binding_families,
    )
}

fn artifact_context_from_sealed(sealed: &SealedArtifact) -> ArtifactContext {
    ArtifactContext::new(
        sealed.binding().clone(),
        sealed.program_id(),
        sealed.static_table_artifact().root,
    )
}

/// Execution backends contributing AIRs, trace chips, and witness kits
/// for a given program shape, in a deterministic registration order.
///
/// Consumed by both the machine builder (which needs AIRs / trace
/// chips) and the proving-time witness driver (which needs the kits
/// published by each backend). Keeping one authoritative selector
/// guarantees machine and witness agree on which backends are live.
pub(crate) fn execution_backends_for(
    uses_ir_hash: bool,
    relation_policy: SealedRelationPolicy,
) -> Vec<Arc<dyn ExecutionBackend>> {
    let mut backends: Vec<Arc<dyn ExecutionBackend>> =
        vec![Arc::new(PublicStatementTranscriptExecutionBackend)];
    if uses_ir_hash {
        backends.push(Arc::new(IrHashExecutionBackend));
    }
    if relation_policy.requires_artifact_root() {
        backends.push(Arc::new(RelationExecutionBackend));
    }
    backends
}

pub(crate) fn build_registered_program_machine(
    shape: &ProgramSetup,
    machine_stark_config: &TabulaStarkConfig,
    root_proof_backend: Arc<dyn RootProofBackend>,
) -> Result<TabulaMachine, RuntimeError> {
    let mut machine_builder = build_machine_builder(machine_stark_config, root_proof_backend)
        .with_columns(shape.machine_columns.iter().cloned());
    for backend in execution_backends_for(shape.uses_ir_hash, shape.relation_policy) {
        machine_builder = attach_execution_backend(machine_builder, backend);
    }
    machine_builder
        .build()
        .map_err(|e| SetupError::MachineSetup(e).into())
}


pub(crate) fn validate_core_first_program(program: &ir::Program) -> Result<(), RuntimeError> {
    for entry in &program.entries {
        for (op_index, op) in entry.body.ops.iter().enumerate() {
            match op {
                ir::Op::ReadStateProperty { .. } => {}
                ir::Op::CallCapability { .. } => {
                    return Err(SetupError::Validation {
                        detail: format!(
                            "entry {} ('{}') op {} ({op:?}) is outside the current native proving subset: capability calls are intentionally fail-closed",
                            entry.id.0, entry.symbol, op_index,
                        ),
                    }
                    .into());
                }
                _ => {}
            }
        }
    }
    Ok(())
}
