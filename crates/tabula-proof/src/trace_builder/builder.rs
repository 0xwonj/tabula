use std::collections::{BTreeMap, BTreeSet};

use p3_baby_bear::BabyBear;

use tabula_commitment::{FieldHasher, NativeDigest};
use tabula_core::error::TabulaError;
use tabula_core::traits::StaticTableProvider;
use tabula_core::{Batch, ColId, ExecutionResult, TableId, TableSchema};
use tabula_ir::Program;

use crate::air::chips::execution::trace::InstructionRecord;
use crate::air::chips::smt_path::trace::{SmtPathWitness, SmtTablePathWitness};
use crate::air::chips::static_table::trace::StaticTableRow;
use crate::witness::BatchWitness;

use super::lowering::lower_program_batch;
use super::memory;
use super::orchestration;
use super::smt::build_smt_paths;
use super::types::{AllTraceBundle, ProofTraceBundle};
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

    /// Build memory/metadata traces.
    pub fn build_memory(&self) -> Result<ProofTraceBundle<W>, TabulaError> {
        memory::build_trace_bundle::<H, W>(self.witness)
    }

    /// Build all chip traces from instruction records.
    pub fn build_all(&self, inputs: AllTraceInputs<'_>) -> Result<AllTraceBundle<W>, TabulaError> {
        orchestration::build_all_trace_bundle::<H, W>(
            self.witness,
            inputs.execution_records,
            inputs.static_table_rows,
            inputs.smt_col_paths,
            inputs.smt_table_paths,
        )
    }

    /// Build all chip traces from access-level execution results.
    pub fn build_all_from_execution_result(
        &self,
        execution_result: &ExecutionResult,
        schemas: &BTreeMap<TableId, TableSchema>,
        static_table_rows: &[StaticTableRow],
        smt_col_paths: &[SmtPathWitness],
        smt_table_paths: &[SmtTablePathWitness],
    ) -> Result<AllTraceBundle<W>, TabulaError> {
        orchestration::build_all_trace_bundle_from_execution_result::<H, W>(
            self.witness,
            execution_result,
            schemas,
            static_table_rows,
            smt_col_paths,
            smt_table_paths,
        )
    }

    /// Validate all chip traces against this witness roots.
    pub fn debug_validate_all(&self, bundle: &AllTraceBundle<W>) -> Result<(), TabulaError> {
        validation::debug_validate_all_trace_bundle::<W>(
            bundle,
            &self.witness.old_state_root,
            &self.witness.new_state_root,
        )
    }

    /// Full pipeline: IR program + execution result → all chip traces.
    ///
    /// 1. Derives `empty_columns` from witness column metas
    /// 2. Lowers program body into `InstructionRecord`s for all opcodes
    /// 3. Builds SMT inclusion-proof paths
    /// 4. Assembles all chip traces via the orchestrator
    pub fn build_all_from_program(
        &self,
        program: &Program,
        batch: &Batch,
        execution_result: &ExecutionResult,
        schemas: &BTreeMap<TableId, TableSchema>,
        static_tables: &dyn StaticTableProvider,
        hasher: H,
    ) -> Result<AllTraceBundle<W>, TabulaError>
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

        // 4. Assemble all chip traces.
        self.build_all(AllTraceInputs {
            execution_records: &lowering.instruction_records,
            static_table_rows: &lowering.static_table_rows,
            smt_col_paths: &smt_col_paths,
            smt_table_paths: &smt_table_paths,
        })
    }
}

/// Build all memory/metadata traces from one `BatchWitness`.
pub fn build_trace_bundle<H, const W: usize>(
    witness: &BatchWitness<H>,
) -> Result<ProofTraceBundle<W>, TabulaError>
where
    H: FieldHasher<F = BabyBear, Digest = NativeDigest>,
{
    TraceBuilder::<H, W>::new(witness).build_memory()
}

/// Build all-chip traces from a single orchestrator entrypoint.
pub fn build_all_trace_bundle<H, const W: usize>(
    witness: &BatchWitness<H>,
    execution_records: &[InstructionRecord],
    static_table_rows: &[StaticTableRow],
    smt_col_paths: &[SmtPathWitness],
    smt_table_paths: &[SmtTablePathWitness],
) -> Result<AllTraceBundle<W>, TabulaError>
where
    H: FieldHasher<F = BabyBear, Digest = NativeDigest>,
{
    TraceBuilder::<H, W>::new(witness).build_all(AllTraceInputs {
        execution_records,
        static_table_rows,
        smt_col_paths,
        smt_table_paths,
    })
}

/// Build all-chip traces directly from `ExecutionResult` via access-event lowering.
pub fn build_all_trace_bundle_from_execution_result<H, const W: usize>(
    witness: &BatchWitness<H>,
    execution_result: &ExecutionResult,
    schemas: &BTreeMap<TableId, TableSchema>,
    static_table_rows: &[StaticTableRow],
    smt_col_paths: &[SmtPathWitness],
    smt_table_paths: &[SmtTablePathWitness],
) -> Result<AllTraceBundle<W>, TabulaError>
where
    H: FieldHasher<F = BabyBear, Digest = NativeDigest>,
{
    TraceBuilder::<H, W>::new(witness).build_all_from_execution_result(
        execution_result,
        schemas,
        static_table_rows,
        smt_col_paths,
        smt_table_paths,
    )
}

/// Validate an all-chip bundle with debug constraints and bus balance checks.
pub fn debug_validate_all_trace_bundle<const W: usize>(
    bundle: &AllTraceBundle<W>,
    old_state_root: &NativeDigest,
    new_state_root: &NativeDigest,
) -> Result<(), TabulaError> {
    validation::debug_validate_all_trace_bundle::<W>(bundle, old_state_root, new_state_root)
}

/// Full pipeline: IR program + execution result → all chip traces.
pub fn build_all_from_program<H, const W: usize>(
    witness: &BatchWitness<H>,
    program: &Program,
    batch: &Batch,
    execution_result: &ExecutionResult,
    schemas: &BTreeMap<TableId, TableSchema>,
    static_tables: &dyn StaticTableProvider,
    hasher: H,
) -> Result<AllTraceBundle<W>, TabulaError>
where
    H: FieldHasher<F = BabyBear, Digest = NativeDigest> + Clone,
{
    TraceBuilder::<H, W>::new(witness).build_all_from_program(
        program,
        batch,
        execution_result,
        schemas,
        static_tables,
        hasher,
    )
}
