use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_artifact::PrecompileDescriptor;
use tabula_core::{ColId, TableId, TableSchema};
use tabula_executor::ResolvedExecutionProgram;
use tabula_ext::backend::precompile::PrecompileProofPreparer;
use tabula_ext::backend::scheme::ColumnProofBackend;
use tabula_ext::{MaterializedColumnBackend, RuntimeColumn};
use tabula_ir::Program;
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry};

use crate::RuntimeError;
use crate::bootstrap::materialize::ResolvedRuntimeColumns;
use crate::program::{Binding, binding_from_compiled_program};

/// Ordered proof-preparation slot for one materialized column backend.
#[derive(Clone)]
pub struct ColumnProofSlot {
    /// Table identifier for this proof slot.
    pub table: TableId,
    /// Column identifier for this proof slot.
    pub col: ColId,
    /// Materialized proof backend for this slot.
    pub proof_backend: Arc<dyn ColumnProofBackend>,
}

/// Ordered proof-preparation slot for one sealed precompile descriptor.
#[derive(Clone)]
pub struct PrecompileProofSlot {
    /// Sealed precompile descriptor for this slot.
    pub descriptor: PrecompileDescriptor,
    /// Materialized proof preparer for this slot.
    pub preparer: Arc<dyn PrecompileProofPreparer>,
}

/// Ordered runtime-owned proof plan aligned to downstream proof preparation.
#[derive(Clone)]
pub struct ProofPlan {
    column_slots: Vec<ColumnProofSlot>,
    precompile_slots: Vec<PrecompileProofSlot>,
}

impl ProofPlan {
    /// Create an ordered proof plan.
    pub fn new(
        column_slots: Vec<ColumnProofSlot>,
        precompile_slots: Vec<PrecompileProofSlot>,
    ) -> Self {
        Self {
            column_slots,
            precompile_slots,
        }
    }

    /// Ordered column proof slots.
    pub fn column_slots(&self) -> &[ColumnProofSlot] {
        &self.column_slots
    }

    /// Ordered precompile proof slots.
    pub fn precompile_slots(&self) -> &[PrecompileProofSlot] {
        &self.precompile_slots
    }
}

/// Runtime-owned proof contract and planning state.
#[derive(Clone)]
pub struct ResolvedProofProgram {
    program: Program,
    schemas_by_id: BTreeMap<TableId, TableSchema>,
    runtime_columns: BTreeMap<(TableId, ColId), Arc<dyn RuntimeColumn>>,
    column_backends: BTreeMap<(TableId, ColId), MaterializedColumnBackend>,
    type_runtimes: TypeRuntimeRegistry,
    encoding_runtimes: EncodingRuntimeRegistry,
    binding: Binding,
    proof_plan: ProofPlan,
}

impl ResolvedProofProgram {
    pub(crate) fn from_compiled_program(
        compiled_program: &tabula_compiler::SealedProgram,
        resolved_columns: ResolvedRuntimeColumns,
        type_runtimes: TypeRuntimeRegistry,
        encoding_runtimes: EncodingRuntimeRegistry,
        proof_plan: ProofPlan,
    ) -> Result<Self, RuntimeError> {
        let binding = binding_from_compiled_program(compiled_program)?;
        let program = compiled_program.program().clone();
        let schemas_by_id = compiled_program
            .table_schemas()
            .iter()
            .cloned()
            .map(|schema| (schema.id, schema))
            .collect();

        Ok(Self {
            program,
            schemas_by_id,
            runtime_columns: resolved_columns.runtime_columns,
            column_backends: resolved_columns.column_backends,
            type_runtimes,
            encoding_runtimes,
            binding,
            proof_plan,
        })
    }

    /// The IR program used for execution statement and proof preparation.
    pub fn program(&self) -> &Program {
        &self.program
    }

    /// Table schemas indexed by `TableId`.
    pub fn schemas_by_id(&self) -> &BTreeMap<TableId, TableSchema> {
        &self.schemas_by_id
    }

    /// Per-column runtime views keyed by `(table_id, col_id)`.
    pub fn runtime_columns(&self) -> &BTreeMap<(TableId, ColId), Arc<dyn RuntimeColumn>> {
        &self.runtime_columns
    }

    /// Per-column materialized backends keyed by `(table_id, col_id)`.
    pub fn column_backends(&self) -> &BTreeMap<(TableId, ColId), MaterializedColumnBackend> {
        &self.column_backends
    }

    /// Runtime type behavior registry used by proof preparation.
    pub fn type_runtimes(&self) -> &TypeRuntimeRegistry {
        &self.type_runtimes
    }

    /// Runtime encoding behavior registry used by witness and transcript assembly.
    pub fn encoding_runtimes(&self) -> &EncodingRuntimeRegistry {
        &self.encoding_runtimes
    }

    /// Precomputed canonical binding for execution statements and proofs.
    pub fn binding(&self) -> &Binding {
        &self.binding
    }

    /// Ordered proof plan used by runtime proving.
    pub fn proof_plan(&self) -> &ProofPlan {
        &self.proof_plan
    }
}

impl std::fmt::Debug for ResolvedProofProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedProofProgram")
            .field("schemas", &self.schemas_by_id.len())
            .field("runtime_columns", &self.runtime_columns.len())
            .field("column_backends", &self.column_backends.len())
            .field("proof_columns", &self.proof_plan.column_slots.len())
            .field("proof_precompiles", &self.proof_plan.precompile_slots.len())
            .field("type_runtimes", &"<registry>")
            .field("encoding_runtimes", &"<registry>")
            .field("program_hash", &self.binding.program_hash())
            .field("metadata_hash", &self.binding.metadata_hash())
            .finish_non_exhaustive()
    }
}

/// Runtime-owned root program contract split into execution and proof subcontracts.
#[derive(Clone, Debug)]
pub struct RuntimeProgram {
    execution: ResolvedExecutionProgram,
    proof: ResolvedProofProgram,
}

impl RuntimeProgram {
    pub(crate) fn from_compiled_program(
        compiled_program: &tabula_compiler::SealedProgram,
        resolved_columns: ResolvedRuntimeColumns,
        type_runtimes: TypeRuntimeRegistry,
        encoding_runtimes: EncodingRuntimeRegistry,
        proof_plan: ProofPlan,
    ) -> Result<Self, RuntimeError> {
        let execution = ResolvedExecutionProgram::from_program(compiled_program.program())
            .map_err(|err| RuntimeError::ValidationFailed {
                detail: err.to_string(),
            })?;
        let proof = ResolvedProofProgram::from_compiled_program(
            compiled_program,
            resolved_columns,
            type_runtimes,
            encoding_runtimes,
            proof_plan,
        )?;
        Ok(Self { execution, proof })
    }

    /// Canonical resolved execution contract.
    pub fn execution(&self) -> &ResolvedExecutionProgram {
        &self.execution
    }

    /// Canonical resolved proof contract.
    pub fn proof(&self) -> &ResolvedProofProgram {
        &self.proof
    }
}
