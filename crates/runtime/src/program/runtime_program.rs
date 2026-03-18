use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_compiler::CompiledProgram;
use tabula_core::{ColId, TableId, TableSchema};
use tabula_ir::Program;

use crate::assembly::materialize::ResolvedColumnViews;
use crate::columns::{ColumnPlan, RuntimeColumn};
#[cfg(feature = "prove")]
use crate::columns::ProofInputBuilder;
use crate::error::RuntimeError;
use crate::program::ProgramBinding;

/// Runtime-owned program state.
///
/// This is the result of resolving compiler-owned proof planning against the
/// installed scheme factories. It keeps only the state the runtime repeatedly
/// needs during execute/prove/verify:
/// - the IR program,
/// - table schemas indexed for witness generation,
/// - per-column runtime column views,
/// - column plans keyed by `(table, col)`,
/// - precomputed artifact-binding hashes.
#[derive(Clone)]
pub struct RuntimeProgram {
    program: Program,
    schemas_by_id: BTreeMap<TableId, TableSchema>,
    runtime_columns: BTreeMap<(TableId, ColId), Arc<dyn RuntimeColumn>>,
    column_plans: BTreeMap<(TableId, ColId), ColumnPlan>,
    #[cfg(feature = "prove")]
    proof_input_builders: BTreeMap<(TableId, ColId), Arc<dyn ProofInputBuilder>>,
    binding: ProgramBinding,
}

impl RuntimeProgram {
    pub(crate) fn from_compiled_program(
        compiled_program: &CompiledProgram,
        resolved_columns: ResolvedColumnViews,
    ) -> Result<Self, RuntimeError> {
        let binding = ProgramBinding::from_compiled_program(compiled_program)?;
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
            column_plans: resolved_columns.column_plans,
            #[cfg(feature = "prove")]
            proof_input_builders: resolved_columns.proof_input_builders,
            binding,
        })
    }

    /// The IR program used for execution.
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

    /// Per-column plans keyed by `(table_id, col_id)`.
    pub fn column_plans(&self) -> &BTreeMap<(TableId, ColId), ColumnPlan> {
        &self.column_plans
    }

    /// Per-column proof-input builders keyed by `(table_id, col_id)`.
    #[cfg(feature = "prove")]
    pub fn proof_input_builders(
        &self,
    ) -> &BTreeMap<(TableId, ColId), Arc<dyn ProofInputBuilder>> {
        &self.proof_input_builders
    }

    /// Precomputed canonical binding for execution statements and proofs.
    pub fn binding(&self) -> &ProgramBinding {
        &self.binding
    }
}

impl std::fmt::Debug for RuntimeProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RuntimeProgram")
            .field("schemas", &self.schemas_by_id.len())
            .field("runtime_columns", &self.runtime_columns.len())
            .field("column_plans", &self.column_plans.len())
            .field(
                "proof_input_builders",
                &{
                    #[cfg(feature = "prove")]
                    {
                        self.proof_input_builders.len()
                    }
                    #[cfg(not(feature = "prove"))]
                    {
                        0usize
                    }
                },
            )
            .field("program_hash", &self.binding.program_hash())
            .field("metadata_hash", &self.binding.metadata_hash())
            .finish_non_exhaustive()
    }
}
