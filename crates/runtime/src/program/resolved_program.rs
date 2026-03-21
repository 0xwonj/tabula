use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_compiler::SealedProgram;
use tabula_core::{ColId, TableId, TableSchema};
use tabula_ir::Program;

use crate::columns::{ResolvedColumnPlan, RuntimeColumn};
use crate::error::RuntimeError;
use crate::program::{Binding, binding_from_compiled_program};
use crate::setup::materialize::ResolvedRuntimeColumns;

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
pub struct ResolvedProgram {
    program: Program,
    schemas_by_id: BTreeMap<TableId, TableSchema>,
    runtime_columns: BTreeMap<(TableId, ColId), Arc<dyn RuntimeColumn>>,
    column_plans: BTreeMap<(TableId, ColId), ResolvedColumnPlan>,
    binding: Binding,
}

impl ResolvedProgram {
    pub(crate) fn from_compiled_program(
        compiled_program: &SealedProgram,
        resolved_columns: ResolvedRuntimeColumns,
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
            column_plans: resolved_columns.column_plans,
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
    pub fn column_plans(&self) -> &BTreeMap<(TableId, ColId), ResolvedColumnPlan> {
        &self.column_plans
    }

    /// Precomputed canonical binding for execution statements and proofs.
    pub fn binding(&self) -> &Binding {
        &self.binding
    }
}

impl std::fmt::Debug for ResolvedProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedProgram")
            .field("schemas", &self.schemas_by_id.len())
            .field("runtime_columns", &self.runtime_columns.len())
            .field("column_plans", &self.column_plans.len())
            .field("program_hash", &self.binding.program_hash())
            .field("metadata_hash", &self.binding.metadata_hash())
            .finish_non_exhaustive()
    }
}
