use std::collections::BTreeMap;

use p3_koala_bear::KoalaBear;

use crate::trace::lowering::LoweringOutput;
use tabula_commitment::{ColumnMeta, FieldHasher, NativeDigest};
use tabula_core::error::TabulaError;
use tabula_core::{ColId, TableId};

use tabula_chips::execution::trace::{InstructionRecord, Opcode};
use tabula_chips::shards::property::trace::PropertyReadRecord;
use tabula_chips::smt_path::trace::{SmtPathWitness, SmtTablePathWitness};
use tabula_chips::static_table::trace::StaticTableRow;

use tabula_stark::trace::{WitnessStore, witness_labels};

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

/// Builtin trace-builder facade for one witness context.
#[derive(Clone, Copy)]
pub struct BuiltinTraceBuilder<'a, H, const W: usize>
where
    H: FieldHasher<F = KoalaBear, Digest = NativeDigest>,
{
    context: BuiltinTraceContext<'a>,
    _marker: core::marker::PhantomData<H>,
}

/// Minimal shared proving context required by builtin execution/root traces.
#[derive(Clone, Copy)]
pub struct BuiltinTraceContext<'a> {
    /// Column metadata for all planned columns.
    pub column_metas: &'a [ColumnMeta],
    /// State root before the batch.
    pub old_state_root: &'a NativeDigest,
    /// State root after the batch.
    pub new_state_root: &'a NativeDigest,
}

/// Builtin witness-store output plus per-column property-read records.
pub struct BuiltinWitnessInputs {
    /// Shared execution/root witness store.
    pub store: WitnessStore,
    /// Property reads grouped per `(table, col)` for runtime-owned proof-input assembly.
    pub property_reads: BTreeMap<(TableId, ColId), Vec<PropertyReadRecord>>,
}

impl<'a, H, const W: usize> BuiltinTraceBuilder<'a, H, W>
where
    H: FieldHasher<F = KoalaBear, Digest = NativeDigest>,
{
    /// Create a new trace builder for a witness.
    pub fn new(context: BuiltinTraceContext<'a>) -> Self {
        Self {
            context,
            _marker: core::marker::PhantomData,
        }
    }

    /// Build the shared execution/root witness store from already-lowered
    /// execution inputs plus the current batch proof context.
    pub fn prepare_witness_store(
        &self,
        lowering: &LoweringOutput,
        hasher: H,
    ) -> Result<BuiltinWitnessInputs, TabulaError>
    where
        H: Clone,
    {
        let (smt_col_paths, smt_table_paths) = super::smt::build_smt_paths(
            self.context.column_metas,
            self.context.old_state_root,
            self.context.new_state_root,
            hasher,
        )?;

        let inputs = AllTraceInputs {
            execution_records: &lowering.instruction_records,
            static_table_rows: &lowering.static_table_rows,
            smt_col_paths: &smt_col_paths,
            smt_table_paths: &smt_table_paths,
        };

        validate_smt_path_shapes(inputs.smt_col_paths, inputs.smt_table_paths)?;
        self.populate_store(inputs)
    }

    /// Populate a [`WitnessStore`] from pre-computed [`AllTraceInputs`].
    ///
    /// Lower-level entry point for callers that already have instruction records,
    /// static table rows, and SMT paths (e.g. chip-level tests).
    /// For the canonical builtin path, use [`prepare_witness_store`](Self::prepare_witness_store).
    pub fn populate_store(
        &self,
        inputs: AllTraceInputs<'_>,
    ) -> Result<BuiltinWitnessInputs, TabulaError> {
        let statement = super::smt::smt_table_public_statement(
            self.context.old_state_root,
            self.context.new_state_root,
        );
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

        // PropertyRead records stay outside the shared store so runtime-owned
        // proof-input builders do not depend on a magic cross-crate label.
        let property_records = extract_property_read_records(inputs.execution_records);

        // Phase 1 (Memory) inputs are handled per-column by prepared column
        // schemes. No global memory chip inputs live in the shared store.
        //
        // Phase 2 (Dependent) inputs are collected by the orchestrator
        // via BusConsumer dispatch between Phase 1 and Phase 2.

        Ok(BuiltinWitnessInputs {
            store,
            property_reads: property_records,
        })
    }
}

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
                query_arg0: rec.property_query_arg0.clone(),
                query_arg1: rec.property_query_arg1.clone(),
                result_val: rec.property_result_val.clone(),
                result_key: rec.property_result_key.clone(),
                is_null: rec.property_result_is_null,
            });
    }
    result
}
