use std::collections::{BTreeMap, BTreeSet};

use p3_baby_bear::BabyBear;

use tabula_commitment::{FieldHasher, NativeDigest};
use tabula_core::error::TabulaError;
use tabula_core::traits::StaticTableProvider;
use tabula_core::{Batch, BatchResult, ColId, TableId, TableSchema};
use tabula_ir::Program;

use crate::witness::BatchWitness;
use tabula_chips::execution::trace::{InstructionRecord, Opcode};
use tabula_chips::shards::property::trace::PropertyReadRecord;
use tabula_chips::smt_path::trace::{SmtPathWitness, SmtTablePathWitness};
use tabula_chips::static_table::trace::StaticTableRow;

use tabula_stark::trace::{WitnessStore, witness_labels};

use super::lowering::lower_program_batch;
use super::smt::validate_smt_path_shapes;

/// Input bundle for all-chip trace construction.
#[derive(Clone, Copy)]
pub struct AllTraceInputs<'a> {
    /// Execution instruction records.
    pub execution_records: &'a [InstructionRecord],
    /// Static table rows.
    pub static_table_rows: &'a [StaticTableRow],
    /// SMT column path witnesses.
    pub smt_col_paths: &'a [SmtPathWitness],
    /// SMT table path witnesses.
    pub smt_table_paths: &'a [SmtTablePathWitness],
}

/// High-level trace-builder facade for one witness context.
#[derive(Clone, Copy)]
pub struct TraceBuilder<'a, H, const W: usize>
where
    H: FieldHasher<F = BabyBear, Digest = NativeDigest>,
{
    witness: &'a BatchWitness<H>,
}

impl<'a, H, const W: usize> TraceBuilder<'a, H, W>
where
    H: FieldHasher<F = BabyBear, Digest = NativeDigest>,
{
    /// Create a new trace builder for a witness.
    pub fn new(witness: &'a BatchWitness<H>) -> Self {
        Self { witness }
    }

    /// Full pipeline: IR program + execution result → populated [`WitnessStore`].
    ///
    /// Runs the complete preparation pipeline:
    /// 1. Derives empty columns from witness metadata.
    /// 2. Lowers IR body → instruction records + static table rows.
    #[allow(clippy::too_many_arguments)]
    /// 3. Builds SMT paths from witness metadata.
    /// 4. Validates SMT path shapes.
    /// 5. Populates a [`WitnessStore`] with all chip inputs.
    pub fn prepare_witness_store(
        &self,
        program: &Program,
        batch: &Batch,
        execution_result: &BatchResult,
        schemas: &BTreeMap<TableId, TableSchema>,
        static_tables: &dyn StaticTableProvider,
        hasher: H,
        precompile_executor: Option<&super::lowering::PrecompileExecuteFn>,
    ) -> Result<WitnessStore, TabulaError>
    where
        H: Clone,
    {
        let prepared = self.prepare_inputs(
            program,
            batch,
            execution_result,
            schemas,
            static_tables,
            hasher,
            precompile_executor,
        )?;

        let inputs = AllTraceInputs {
            execution_records: &prepared.instruction_records,
            static_table_rows: &prepared.static_table_rows,
            smt_col_paths: &prepared.smt_col_paths,
            smt_table_paths: &prepared.smt_table_paths,
        };

        validate_smt_path_shapes(inputs.smt_col_paths, inputs.smt_table_paths)?;
        self.populate_store(inputs)
    }

    /// Populate a [`WitnessStore`] from pre-computed [`AllTraceInputs`].
    ///
    /// Lower-level entry point for callers that already have instruction records,
    /// static table rows, and SMT paths (e.g. chip-level tests).
    /// For the full IR-based pipeline, use [`prepare_witness_store`](Self::prepare_witness_store).
    pub fn populate_store(&self, inputs: AllTraceInputs<'_>) -> Result<WitnessStore, TabulaError> {
        let statement = super::smt::smt_table_public_statement(self.witness);
        let smt_table_pvs = statement.to_field_elements();

        let mut store = WitnessStore::new();

        // Phase 0 (Independent) chip inputs.
        store.put(
            witness_labels::EXECUTION_RECORDS,
            inputs.execution_records.to_vec(),
        );
        store.put(
            witness_labels::STATIC_TABLE_ROWS,
            inputs.static_table_rows.to_vec(),
        );
        store.put(witness_labels::SMT_COL_PATHS, inputs.smt_col_paths.to_vec());
        store.put(
            witness_labels::SMT_TABLE_PATHS,
            inputs.smt_table_paths.to_vec(),
        );
        store.put(witness_labels::SMT_TABLE_PVS, smt_table_pvs);

        // PropertyRead records grouped by (table, col) for column-tier verifiers.
        let property_records = extract_property_read_records(inputs.execution_records);
        store.put(PROPERTY_READ_ALL_LABEL, property_records);

        // Phase 1 (Memory) inputs are handled per-column via shard witness
        // preparation (prepare_shard_witness). No global memory chip inputs.
        //
        // Phase 2 (Dependent) inputs are collected by the orchestrator
        // via BusConsumer dispatch between Phase 1 and Phase 2.

        Ok(store)
    }

    /// Shared input preparation for IR-based pipelines.
    #[allow(clippy::too_many_arguments)]
    fn prepare_inputs(
        &self,
        program: &Program,
        batch: &Batch,
        execution_result: &BatchResult,
        schemas: &BTreeMap<TableId, TableSchema>,
        static_tables: &dyn StaticTableProvider,
        hasher: H,
        precompile_executor: Option<&super::lowering::PrecompileExecuteFn>,
    ) -> Result<PreparedInputs, TabulaError>
    where
        H: Clone,
    {
        // 1. Derive empty columns from witness metadata.
        let empty_columns: BTreeSet<(TableId, ColId)> = self
            .witness
            .column_metas
            .iter()
            .filter(|m| m.is_empty_old)
            .map(|m| (m.table, m.col))
            .collect();

        // 2. Lower IR body → instruction records + static table rows.
        let lowering = lower_program_batch::<W>(
            program,
            batch,
            execution_result,
            schemas,
            static_tables,
            &empty_columns,
            precompile_executor,
            None,
        )?;

        // 3. Build SMT paths from witness metadata.
        let (smt_col_paths, smt_table_paths) = super::smt::build_smt_paths(
            &self.witness.column_metas,
            &self.witness.old_state_root,
            &self.witness.new_state_root,
            hasher,
        )?;

        Ok(PreparedInputs {
            instruction_records: lowering.instruction_records,
            static_table_rows: lowering.static_table_rows,
            smt_col_paths,
            smt_table_paths,
        })
    }
}

/// Internal bundle of prepared inputs for trace assembly.
struct PreparedInputs {
    instruction_records: Vec<InstructionRecord>,
    static_table_rows: Vec<StaticTableRow>,
    smt_col_paths: Vec<SmtPathWitness>,
    smt_table_paths: Vec<SmtTablePathWitness>,
}

/// WitnessStore label for the global PropertyRead records map.
///
/// Contains `BTreeMap<(TableId, ColId), Vec<PropertyReadRecord>>` that gets
/// split per-column during partitioning.
pub(crate) const PROPERTY_READ_ALL_LABEL: &str = "property_read_all";

/// Extract PropertyRead records from instruction records, grouped by (table, col).
fn extract_property_read_records(
    records: &[InstructionRecord],
) -> BTreeMap<(TableId, ColId), Vec<PropertyReadRecord>> {
    let mut result: BTreeMap<(TableId, ColId), Vec<PropertyReadRecord>> = BTreeMap::new();
    for rec in records {
        if rec.opcode != Opcode::PropertyRead {
            continue;
        }
        let table = TableId(rec.access_t.unwrap_or(0));
        let col = ColId(rec.access_c.unwrap_or(0));
        result
            .entry((table, col))
            .or_default()
            .push(PropertyReadRecord {
                query_type: rec.property_query_type.unwrap_or(0),
                result_val: rec.property_result_val.clone(),
                result_key: rec.property_result_key.clone(),
                is_null: rec.property_result_is_null,
            });
    }
    result
}
