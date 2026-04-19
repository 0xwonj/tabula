//! Central prepared-runtime carrier shared by all prepared handles.
//!
//! Every extracted runtime module touches [`PreparedRuntimeState`], so this
//! module owns it (together with its verify-gated build sibling
//! [`PreparedRuntimeBuild`], the verify-gated [`build_prepared_runtime`]
//! constructor, and the prove-gated [`build_chip_kit_registry`] helper)
//! so every other module imports from one place.

use std::sync::Arc;

use tabula_compiler::RegisteredProgram;
#[cfg(feature = "prove")]
use tabula_contract::TupleEncodingDefaults;
use tabula_contract::{ArtifactContext, SealedRelationPolicy, StaticTableArtifact};
#[cfg(feature = "prove")]
use tabula_core::{ColId, TableId};
#[cfg(feature = "prove")]
use tabula_ext::root::RootBackendBundle;
#[cfg(all(feature = "verify", not(feature = "prove")))]
use tabula_ext::root::RootProofBackend;
use tabula_machine::{TabulaMachine, TabulaStarkConfig};
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry};
#[cfg(feature = "prove")]
use tabula_witness::stark::ChipKitRegistry;

use crate::bootstrap::program::{
    build_registered_program_machine, resolve_program_setup, validate_core_first_program,
};
use crate::error::{RuntimeError, SetupError};
use crate::host::HostEnvironment;
use crate::semantics as runtime_ir;
use crate::state_runtime::ResolvedStateRuntime;

/// Per-column proof-backend slot carried through the prepared runtime state.
#[cfg(feature = "prove")]
#[derive(Clone)]
pub(crate) struct ColumnProofSlot {
    /// Table ID for this column slot.
    pub(crate) table: TableId,
    /// Column ID for this column slot.
    pub(crate) col: ColId,
    /// Proof backend for this column.
    pub(crate) proof_backend: Arc<dyn tabula_ext::backend::column::ColumnProofBackend>,
}

/// Prepared runtime state derived from a registered program.
///
/// Shared between `TabulaRuntime` (execute surface) and `PreparedProver`
/// (prove surface). Construction is feature-gated; the fields marked
/// `#[cfg(feature = "prove")]` are carried only on the prove build.
#[derive(Clone)]
pub(crate) struct PreparedRuntimeState {
    pub(crate) semantic: runtime_ir::RuntimeProgram,
    pub(crate) state: ResolvedStateRuntime,
    #[cfg(feature = "prove")]
    pub(crate) column_slots: Vec<ColumnProofSlot>,
    pub(crate) artifact_context: ArtifactContext,
    pub(crate) relation_policy: SealedRelationPolicy,
    #[cfg(feature = "prove")]
    pub(crate) uses_ir_hash: bool,
    pub(crate) static_table_artifact: StaticTableArtifact,
    #[cfg(feature = "prove")]
    pub(crate) tuple_encoding_defaults: TupleEncodingDefaults,
    pub(crate) type_runtimes: TypeRuntimeRegistry,
    pub(crate) encoding_runtimes: EncodingRuntimeRegistry,
}

/// Output of `build_prepared_runtime`: the prepared state plus machine and, on prove builds,
/// the root-backend bundle.
#[cfg(feature = "verify")]
pub(crate) struct PreparedRuntimeBuild {
    pub(crate) runtime_program: PreparedRuntimeState,
    pub(crate) machine: TabulaMachine,
    #[cfg(feature = "prove")]
    pub(crate) root_backend_bundle: RootBackendBundle,
}

/// Build the [`ChipKitRegistry`] derived from a prepared runtime state.
///
/// This runs once at handle-build time (shared between `TabulaRuntime`
/// and `PreparedProver`). Per-prove work must still allocate a fresh
/// `KitScratch` — see SP-4 §2.5 on the SP-3 boundary.
#[cfg(feature = "prove")]
pub(crate) fn build_chip_kit_registry(state: &PreparedRuntimeState) -> ChipKitRegistry {
    let mut kit_registry = ChipKitRegistry::new();
    for backend in
        crate::bootstrap::program::execution_backends_for(state.uses_ir_hash, state.relation_policy)
    {
        kit_registry.register_all(backend.witness_kits());
    }
    kit_registry
}

/// Shared factory that constructs the prepared runtime state consumed by both the execute
/// and prove surfaces.
#[cfg(feature = "verify")]
pub(crate) fn build_prepared_runtime(
    registered_program: &RegisteredProgram,
    host_environment: &HostEnvironment,
    machine_stark_config: &TabulaStarkConfig,
    #[cfg(feature = "prove")] root_backend_bundle: RootBackendBundle,
    #[cfg(not(feature = "prove"))] root_proof_backend: Arc<dyn RootProofBackend>,
) -> Result<PreparedRuntimeBuild, RuntimeError> {
    validate_core_first_program(registered_program.program())?;
    let type_runtimes = host_environment
        .runtime_registries()
        .type_runtimes()
        .clone();
    let encoding_runtimes = host_environment
        .runtime_registries()
        .encoding_runtimes()
        .clone();
    #[cfg(feature = "prove")]
    let proof_backend = root_backend_bundle.proof_backend();
    #[cfg(not(feature = "prove"))]
    let proof_backend = Arc::clone(&root_proof_backend);
    #[cfg(feature = "prove")]
    let accepted_root_binding_families = root_backend_bundle.supported_root_binding_families();
    #[cfg(not(feature = "prove"))]
    let accepted_root_binding_families = proof_backend.supported_root_binding_families();
    let program_setup = resolve_program_setup(
        registered_program,
        host_environment.schemes().factories(),
        &type_runtimes,
        &encoding_runtimes,
        accepted_root_binding_families,
    )?;
    #[cfg(feature = "prove")]
    let column_slots = program_setup
        .resolved_state
        .backends()
        .map(|backend| ColumnProofSlot {
            table: backend.table_id,
            col: backend.col_id,
            proof_backend: Arc::clone(&backend.proof_backend),
        })
        .collect::<Vec<_>>();

    let semantic = runtime_ir::RuntimeProgram::from_validated_program(
        registered_program.validated_program().clone(),
    )
    .map_err(|error| SetupError::Validation {
        detail: error.to_string(),
    })?;

    let machine =
        build_registered_program_machine(&program_setup, machine_stark_config, proof_backend)?;

    let runtime_program = PreparedRuntimeState {
        semantic,
        state: program_setup.resolved_state.clone(),
        #[cfg(feature = "prove")]
        column_slots,
        artifact_context: program_setup.artifact_context,
        relation_policy: program_setup.relation_policy,
        #[cfg(feature = "prove")]
        uses_ir_hash: program_setup.uses_ir_hash,
        static_table_artifact: registered_program.static_table_artifact().clone(),
        #[cfg(feature = "prove")]
        tuple_encoding_defaults: registered_program.tuple_encoding_defaults().clone(),
        type_runtimes,
        encoding_runtimes,
    };

    Ok(PreparedRuntimeBuild {
        runtime_program,
        machine,
        #[cfg(feature = "prove")]
        root_backend_bundle,
    })
}
