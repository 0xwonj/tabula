use std::sync::Arc;

use tabula_compiler::RegisteredProgram;
use tabula_contract::ArtifactContext;
use tabula_core::RootProfileId;
use tabula_ext::backend::ProofColumn;
use tabula_ext::backend::execution::{
    IrHashExecutionBackend, PublicStatementTranscriptExecutionBackend, RelationExecutionBackend,
};
use tabula_ext::root::RootProofBackend;
use tabula_ir as ir;
use tabula_machine::{TabulaMachine, TabulaStarkConfig};
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry};

use crate::bootstrap::machine::{attach_execution_backend, build_machine_builder};
use crate::error::RuntimeError;
use crate::host::SchemeFactoryMap;
use crate::state_runtime::ResolvedStateRuntime;

/// Verifier-side relation policy derived from the sealed program.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RelationPolicy {
    Disabled,
    RequireArtifactRoot,
}

impl RelationPolicy {
    pub(crate) fn from_program(program: &ir::Program) -> Self {
        if program_uses_relations(program) {
            Self::RequireArtifactRoot
        } else {
            Self::Disabled
        }
    }

    pub(crate) const fn requires_artifact_root(self) -> bool {
        matches!(self, Self::RequireArtifactRoot)
    }
}

/// Shared registered-program setup used by both runtime proving and verification.
#[derive(Clone)]
pub(crate) struct ProgramSetup {
    pub(crate) artifact_context: ArtifactContext,
    pub(crate) resolved_state: ResolvedStateRuntime,
    pub(crate) machine_columns: Vec<Arc<dyn ProofColumn>>,
    pub(crate) relation_policy: RelationPolicy,
    pub(crate) uses_ir_hash: bool,
}

pub(crate) fn resolve_program_setup(
    registered_program: &RegisteredProgram,
    backend_factories: &SchemeFactoryMap,
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
    accepted_root_binding_families: &[RootProfileId],
) -> Result<ProgramSetup, RuntimeError> {
    let resolved_state = materialize_registered_state_runtime(
        registered_program,
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
        artifact_context: artifact_context_from_registered_program(registered_program),
        resolved_state,
        machine_columns,
        relation_policy: RelationPolicy::from_program(registered_program.program()),
        uses_ir_hash: program_uses_hash(registered_program.program()),
    })
}

fn artifact_context_from_registered_program(
    registered_program: &RegisteredProgram,
) -> ArtifactContext {
    ArtifactContext::new(
        registered_program.binding().clone(),
        registered_program.program().program_id,
        registered_program.static_table_artifact().root,
    )
}

pub(crate) fn build_registered_program_machine(
    shape: &ProgramSetup,
    machine_stark_config: &TabulaStarkConfig,
    root_proof_backend: Arc<dyn RootProofBackend>,
) -> Result<TabulaMachine, RuntimeError> {
    let mut machine_builder = build_machine_builder(machine_stark_config, root_proof_backend)
        .with_columns(shape.machine_columns.iter().cloned());
    machine_builder = attach_execution_backend(
        machine_builder,
        Arc::new(PublicStatementTranscriptExecutionBackend),
    );
    if shape.uses_ir_hash {
        machine_builder =
            attach_execution_backend(machine_builder, Arc::new(IrHashExecutionBackend));
    }
    if shape.relation_policy.requires_artifact_root() {
        machine_builder =
            attach_execution_backend(machine_builder, Arc::new(RelationExecutionBackend));
    }
    machine_builder.build().map_err(RuntimeError::MachineSetup)
}

pub(crate) fn materialize_registered_state_runtime(
    registered_program: &RegisteredProgram,
    backend_factories: &SchemeFactoryMap,
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
    accepted_root_binding_families: &[RootProfileId],
) -> Result<ResolvedStateRuntime, RuntimeError> {
    ResolvedStateRuntime::from_registered_program(
        registered_program,
        backend_factories,
        type_runtimes,
        encoding_runtimes,
        accepted_root_binding_families,
    )
}

pub(crate) fn validate_core_first_program(program: &ir::Program) -> Result<(), RuntimeError> {
    for entry in &program.entries {
        for (op_index, op) in entry.body.ops.iter().enumerate() {
            match op {
                ir::Op::ReadStateProperty { .. } => {}
                ir::Op::CallCapability { .. } => {
                    return Err(RuntimeError::ValidationFailed {
                        detail: format!(
                            "entry {} ('{}') op {} ({op:?}) is outside the current native proving subset: capability calls are intentionally fail-closed",
                            entry.id.0, entry.symbol, op_index,
                        ),
                    });
                }
                _ => {}
            }
        }
    }
    Ok(())
}

pub(crate) fn program_uses_hash(program: &ir::Program) -> bool {
    program
        .entries
        .iter()
        .flat_map(|entry| entry.body.ops.iter())
        .any(|op| matches!(op, ir::Op::Hash { .. }))
}

pub(crate) fn program_uses_relations(program: &ir::Program) -> bool {
    program
        .entries
        .iter()
        .flat_map(|entry| entry.body.ops.iter())
        .any(|op| {
            matches!(
                op,
                ir::Op::AssertRelation { .. } | ir::Op::EvalRelation { .. }
            )
        })
}
