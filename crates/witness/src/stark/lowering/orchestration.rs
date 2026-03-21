//! Batch-level lowering orchestration.
//!
//! Public entry points for lowering execution results to AIR trace records.

use std::collections::{BTreeMap, BTreeSet};

use tabula_commitment::KoalaBearCodec;
use tabula_core::error::TabulaError;
use tabula_core::traits::StaticTableProvider;
use tabula_core::{Batch, BatchResult, ColId, TableId, TableSchema, TxResult};
use tabula_ir::Program;

use tabula_chips::execution::trace::InstructionRecord;
use tabula_chips::static_table::trace::StaticTableRow;

use super::context::LoweringContext;
use super::{build_type_map, lower_tx_body};

/// Output of full program lowering.
#[derive(Debug, Clone)]
pub struct LoweringOutput {
    /// Instruction records for all opcodes across all successful txs.
    pub instruction_records: Vec<InstructionRecord>,
    /// Static table rows accumulated from Lookup instructions.
    pub static_table_rows: Vec<StaticTableRow>,
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
    program: &Program,
    batch: &Batch,
    result: &BatchResult,
    schemas: &BTreeMap<TableId, TableSchema>,
    static_tables: &dyn StaticTableProvider,
    empty_columns: &BTreeSet<(TableId, ColId)>,
) -> Result<LoweringOutput, TabulaError> {
    let type_map = build_type_map(schemas);
    let codec = KoalaBearCodec;

    let mut all_records = Vec::new();
    let mut all_static_rows: BTreeMap<(u32, u16, u64), StaticTableRow> = BTreeMap::new();

    for (tx_idx, tx) in batch.transactions.iter().enumerate() {
        let tx_index = tx_idx as u32;

        // Skip failed txs; extract data from successful ones.
        let (tx_events, precompile_events, property_reads) = match &result.txs[tx_idx] {
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

        let tx_def = program.resolve(tx.tx_type)?;

        let mut ctx = LoweringContext::<W>::new(
            tx_index,
            &tx_events,
            &type_map,
            static_tables,
            empty_columns,
            &tx.params,
            &codec,
            tx_def.body.len(),
            precompile_events,
            property_reads,
        )?;

        lower_tx_body(&mut ctx, &tx_def.body)?;

        let (records, static_rows) = ctx.into_output();
        all_records.extend(records);

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
    })
}
