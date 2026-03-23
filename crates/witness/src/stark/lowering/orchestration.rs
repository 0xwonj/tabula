//! Batch-level lowering orchestration.
//!
//! Public entry points for lowering execution results to AIR trace records.

use std::collections::{BTreeMap, BTreeSet};

use tabula_core::error::TabulaError;
use tabula_core::traits::StaticTableProvider;
use tabula_core::{Batch, BatchResult, ColId, TableId, TableSchema, TxResult};
use tabula_ir::Program;
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry};

use tabula_chips::execution::trace::InstructionRecord;
use tabula_chips::ir_hash::IrHashCall;
use tabula_chips::static_table::trace::StaticTableRow;

use super::context::LoweringContext;
use super::{build_profile_map, lower_tx_body};

/// Output of full program lowering.
#[derive(Debug, Clone)]
pub struct LoweringOutput {
    /// Instruction records for all opcodes across all successful txs.
    pub instruction_records: Vec<InstructionRecord>,
    /// Static table rows accumulated from Lookup instructions.
    pub static_table_rows: Vec<StaticTableRow>,
    /// Canonical IR-hash calls consumed by the dedicated hash lane.
    pub ir_hash_calls: Vec<IrHashCall>,
}

/// Input bundle for lowering one executed batch into witness-ready trace records.
#[derive(Clone, Copy)]
pub struct LowerProgramBatchInput<'a> {
    /// Sealed program whose transactions are being lowered.
    pub program: &'a Program,
    /// Executed batch carrying the submitted transactions.
    pub batch: &'a Batch,
    /// Runtime execution result for the batch.
    pub result: &'a BatchResult,
    /// Table schemas keyed by table id.
    pub schemas: &'a BTreeMap<TableId, TableSchema>,
    /// Installed type runtimes used for typed semantics.
    pub type_runtimes: &'a TypeRuntimeRegistry,
    /// Installed encoding runtimes used for witness encoding.
    pub encoding_runtimes: &'a EncodingRuntimeRegistry,
    /// Static table provider used by lookup lowering.
    pub static_tables: &'a dyn StaticTableProvider,
    /// Columns known to be empty in the old state.
    pub empty_columns: &'a BTreeSet<(TableId, ColId)>,
}

/// Lower a full batch execution from IR programs.
///
/// Walks each successful tx's IR body, producing `InstructionRecord`s
/// for ALL opcodes and collecting `StaticTableRow` entries from Lookups.
///
/// **Limitation**: All `ValueExpr` operands that require slot linkage
/// (src1/src2/cond) must reference either a `Slot(s)` or a value already
/// present in a slot. `Param`/`Literal` operands will search existing
/// slots for a matching value; if none is found, an error is returned.
pub fn lower_program_batch<const W: usize>(
    input: LowerProgramBatchInput<'_>,
) -> Result<LoweringOutput, TabulaError> {
    let profile_map = build_profile_map(
        input.schemas,
        input.program.profile_catalog(),
        input.type_runtimes,
        input.encoding_runtimes,
    )?;

    let mut all_records = Vec::new();
    let mut all_static_rows: BTreeMap<(u32, u16, u64), StaticTableRow> = BTreeMap::new();
    let mut all_ir_hash_calls = Vec::new();

    for (tx_idx, tx) in input.batch.transactions.iter().enumerate() {
        let tx_index = tx_idx as u32;

        // Skip failed txs; extract data from successful ones.
        let (tx_events, precompile_events, property_reads) = match &input.result.txs[tx_idx] {
            TxResult::Success {
                access_trace,
                precompile_events,
                property_reads,
                ..
            } => (
                access_trace.iter().collect::<Vec<_>>(),
                precompile_events.as_slice(),
                property_reads.as_slice(),
            ),
            TxResult::Failed { .. } => continue,
        };

        let tx_def = input.program.resolve(tx.tx_type)?;

        let mut ctx = LoweringContext::<W>::new(
            tx_index,
            &tx_events,
            &profile_map,
            input.type_runtimes,
            input.encoding_runtimes,
            input.static_tables,
            input.empty_columns,
            tx.params.as_slice(),
            input.program.precompiles(),
            tx_def.body.len(),
            precompile_events,
            property_reads,
        )?;

        lower_tx_body(&mut ctx, &tx_def.body)?;

        let (records, static_rows, ir_hash_calls) = ctx.into_output();
        all_records.extend(records);
        all_ir_hash_calls.extend(ir_hash_calls);

        // Merge static table rows (accumulate multiplicities).
        for row in static_rows {
            let key = (row.table_id, row.col_id, row.row_key);
            all_static_rows
                .entry(key)
                .and_modify(|existing| existing.lookup_mult += row.lookup_mult)
                .or_insert(row);
        }
    }

    Ok(LoweringOutput {
        instruction_records: all_records,
        static_table_rows: all_static_rows.into_values().collect(),
        ir_hash_calls: all_ir_hash_calls,
    })
}
