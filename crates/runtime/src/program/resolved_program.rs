use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_compiler::SealedProgram;
use tabula_core::{ColId, TableId, TableSchema};
use tabula_ext::{MaterializedColumnBackend, RuntimeColumn};
use tabula_ir::Program;
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry};

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
/// - materialized column backends keyed by `(table, col)`,
/// - precomputed artifact-binding hashes.
#[derive(Clone)]
pub struct ResolvedProgram {
    program: Program,
    schemas_by_id: BTreeMap<TableId, TableSchema>,
    runtime_columns: BTreeMap<(TableId, ColId), Arc<dyn RuntimeColumn>>,
    column_backends: BTreeMap<(TableId, ColId), MaterializedColumnBackend>,
    type_runtimes: TypeRuntimeRegistry,
    encoding_runtimes: EncodingRuntimeRegistry,
    binding: Binding,
}

impl ResolvedProgram {
    pub(crate) fn from_compiled_program(
        compiled_program: &SealedProgram,
        resolved_columns: ResolvedRuntimeColumns,
        type_runtimes: TypeRuntimeRegistry,
        encoding_runtimes: EncodingRuntimeRegistry,
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

    /// Per-column materialized backends keyed by `(table_id, col_id)`.
    pub fn column_backends(&self) -> &BTreeMap<(TableId, ColId), MaterializedColumnBackend> {
        &self.column_backends
    }

    /// Runtime type behavior registry used by execution/proof preparation.
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
}

impl std::fmt::Debug for ResolvedProgram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResolvedProgram")
            .field("schemas", &self.schemas_by_id.len())
            .field("runtime_columns", &self.runtime_columns.len())
            .field("column_backends", &self.column_backends.len())
            .field("type_runtimes", &"<registry>")
            .field("encoding_runtimes", &"<registry>")
            .field("program_hash", &self.binding.program_hash())
            .field("metadata_hash", &self.binding.metadata_hash())
            .finish_non_exhaustive()
    }
}
