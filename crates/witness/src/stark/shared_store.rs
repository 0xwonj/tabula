//! Shared execution/root witness-store assembly for the current STARK backend.

use p3_koala_bear::KoalaBear;

use tabula_commitment::{ColumnRootBinding, FieldHasher, NativeDigest};
use tabula_core::error::TabulaError;

use tabula_chips::execution::trace::InstructionRecord;
use tabula_chips::ir_hash::{IR_HASH_WITNESS_LABEL, IrHashCall};
use tabula_chips::smt_path::trace::{SmtPathWitness, SmtTablePathWitness};
use tabula_chips::static_table::trace::StaticTableRow;
use tabula_stark::trace::{WitnessStore, witness_labels};

use super::lowering::LoweringOutput;
use super::root_paths::{build_smt_paths, smt_table_public_statement, validate_smt_path_shapes};

/// Input bundle for all-chip trace construction.
#[derive(Clone, Copy)]
struct AllTraceInputs<'a> {
    /// Execution instruction records.
    pub execution_records: &'a [InstructionRecord],
    /// Static table rows.
    pub static_table_rows: &'a [StaticTableRow],
    /// Canonical IR hash calls.
    pub ir_hash_calls: &'a [IrHashCall],
    /// SMT column path witnesses.
    pub smt_col_paths: &'a [SmtPathWitness],
    /// SMT table path witnesses.
    pub smt_table_paths: &'a [SmtTablePathWitness],
}

/// Shared store builder for one STARK witness context.
#[derive(Clone, Copy)]
pub struct SharedStoreBuilder<'a, H, const W: usize>
where
    H: FieldHasher<F = KoalaBear, Digest = NativeDigest>,
{
    context: SharedStoreContext<'a>,
    _marker: core::marker::PhantomData<H>,
}

/// Minimal shared proving context required by execution/root traces.
#[derive(Clone, Copy)]
pub struct SharedStoreContext<'a> {
    /// Column metadata for all planned columns.
    pub column_root_bindings: &'a [ColumnRootBinding],
    /// State root before the batch.
    pub old_state_root: &'a NativeDigest,
    /// State root after the batch.
    pub new_state_root: &'a NativeDigest,
}

impl<'a, H, const W: usize> SharedStoreBuilder<'a, H, W>
where
    H: FieldHasher<F = KoalaBear, Digest = NativeDigest>,
{
    /// Create a new trace builder for a witness.
    pub fn new(context: SharedStoreContext<'a>) -> Self {
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
    ) -> Result<WitnessStore, TabulaError>
    where
        H: Clone,
    {
        let (smt_col_paths, smt_table_paths) = build_smt_paths(
            self.context.column_root_bindings,
            self.context.old_state_root,
            self.context.new_state_root,
            hasher,
        )?;

        let inputs = AllTraceInputs {
            execution_records: &lowering.instruction_records,
            static_table_rows: &lowering.static_table_rows,
            ir_hash_calls: &lowering.ir_hash_calls,
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
    /// For the canonical STARK path, use [`prepare_witness_store`](Self::prepare_witness_store).
    fn populate_store(&self, inputs: AllTraceInputs<'_>) -> Result<WitnessStore, TabulaError> {
        let statement =
            smt_table_public_statement(self.context.old_state_root, self.context.new_state_root);
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
        store.put(IR_HASH_WITNESS_LABEL, inputs.ir_hash_calls.to_vec());
        store.put(witness_labels::SMT_COL_PATHS, inputs.smt_col_paths.to_vec());
        store.put(
            witness_labels::SMT_TABLE_PATHS,
            inputs.smt_table_paths.to_vec(),
        );
        store.put(witness_labels::SMT_TABLE_PVS, smt_table_pvs);

        // Phase 1 (Memory) inputs are handled per-column by prepared column
        // schemes. No global memory chip inputs live in the shared store.
        //
        // Phase 2 (Dependent) inputs are collected by the orchestrator
        // via BusConsumer dispatch between Phase 1 and Phase 2.

        Ok(store)
    }
}
