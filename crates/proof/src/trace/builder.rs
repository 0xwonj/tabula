use std::collections::{BTreeMap, BTreeSet};

use p3_baby_bear::BabyBear;

use tabula_commitment::{FieldHasher, NativeDigest};
use tabula_core::error::TabulaError;
use tabula_core::traits::StaticTableProvider;
use tabula_core::{Batch, ColId, ExecutionResult, TableId, TableSchema};
use tabula_ir::Program;

use crate::chips::execution::trace::InstructionRecord;
use crate::chips::smt_path::trace::{SmtPathWitness, SmtTablePathWitness};
use crate::chips::static_table::trace::StaticTableRow;
use crate::witness::BatchWitness;

use super::lowering::lower_program_batch;
use super::orchestration;
use super::smt::build_smt_paths;
use super::trace_map::TraceMap;
use super::validation;

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

    /// Build all chip traces into a [`TraceMap`].
    pub fn build_all_traces(&self, inputs: AllTraceInputs<'_>) -> Result<TraceMap, TabulaError> {
        orchestration::build_all_traces::<H, W>(
            self.witness,
            inputs.execution_records,
            inputs.static_table_rows,
            inputs.smt_col_paths,
            inputs.smt_table_paths,
        )
    }

    /// Full pipeline: IR program + execution result → [`TraceMap`].
    ///
    /// Builds all chip traces and sets SmtTablePath public values automatically.
    pub fn build_trace_map(
        &self,
        program: &Program,
        batch: &Batch,
        execution_result: &ExecutionResult,
        schemas: &BTreeMap<TableId, TableSchema>,
        static_tables: &dyn StaticTableProvider,
        hasher: H,
    ) -> Result<TraceMap, TabulaError>
    where
        H: Clone,
    {
        let inputs = self.prepare_inputs(
            program,
            batch,
            execution_result,
            schemas,
            static_tables,
            hasher,
        )?;
        orchestration::build_all_traces::<H, W>(
            self.witness,
            &inputs.instruction_records,
            &inputs.static_table_rows,
            &inputs.smt_col_paths,
            &inputs.smt_table_paths,
        )
    }

    /// Validate all chip traces in a [`TraceMap`] against this witness's state roots.
    pub fn debug_validate(&self, map: &TraceMap) -> Result<(), TabulaError> {
        validation::debug_validate_trace_map::<W>(
            map,
            &self.witness.old_state_root,
            &self.witness.new_state_root,
        )
    }

    /// Shared input preparation for IR-based pipelines.
    fn prepare_inputs(
        &self,
        program: &Program,
        batch: &Batch,
        execution_result: &ExecutionResult,
        schemas: &BTreeMap<TableId, TableSchema>,
        static_tables: &dyn StaticTableProvider,
        hasher: H,
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
        )?;

        // 3. Build SMT paths from witness metadata.
        let (smt_col_paths, smt_table_paths) = build_smt_paths(
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

/// Full pipeline: IR program + execution result → [`TraceMap`].
///
/// Convenience wrapper around [`TraceBuilder::build_trace_map`].
pub fn build_trace_map<H, const W: usize>(
    witness: &BatchWitness<H>,
    program: &Program,
    batch: &Batch,
    execution_result: &ExecutionResult,
    schemas: &BTreeMap<TableId, TableSchema>,
    static_tables: &dyn StaticTableProvider,
    hasher: H,
) -> Result<TraceMap, TabulaError>
where
    H: FieldHasher<F = BabyBear, Digest = NativeDigest> + Clone,
{
    TraceBuilder::<H, W>::new(witness).build_trace_map(
        program,
        batch,
        execution_result,
        schemas,
        static_tables,
        hasher,
    )
}
