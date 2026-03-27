//! Next-native witness lowering from canonical `tabula_ir` execution.
use std::collections::{BTreeMap, BTreeSet};

use tabula_chips::execution::trace::InstructionRecord;
use tabula_chips::ir_hash::IrHashCall;
use tabula_chips::relation_transcript::RelationTranscriptCall;
use tabula_chips::static_table::trace::StaticTableRow;
use tabula_contract::format::typed_tuple::TupleEncodingDefaults;
use tabula_core::error::TabulaError;
use tabula_core::traits::Hasher;
use tabula_executor as exec;
use tabula_ir as ir;
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry};

use super::context::LoweringCx;
use crate::RelationClaim;

/// Input bundle for lowering one successful native transaction.
#[derive(Clone, Copy)]
pub struct LowerSuccessfulTxInput<'a> {
    /// Zero-based transaction index within the batch.
    pub tx_index: u32,
    /// Canonical program containing the entry being lowered.
    pub program: &'a ir::Program,
    /// Resolved transaction call.
    pub call: &'a exec::TxCall,
    /// Entry definition being lowered.
    pub entry: &'a ir::Entry,
    /// Execution context values.
    pub context: &'a exec::ContextValues,
    /// Proof-relevant state effects emitted by the executor.
    pub state_effects: &'a [exec::TypedStateEffect],
    /// Proof-relevant event effects emitted by the executor.
    pub event_effects: &'a [exec::TypedEventEffect],
    /// Proof-relevant relation effects emitted by the executor.
    pub relation_effects: &'a [exec::RelationEffect],
    /// Columns known to be empty in the committed pre-state.
    pub empty_columns: &'a BTreeSet<(ir::TableId, ir::FieldId)>,
    /// Installed type runtimes used for typed semantics.
    pub type_runtimes: &'a TypeRuntimeRegistry,
    /// Installed encoding runtimes used for execution-lane witness encoding.
    pub encoding_runtimes: &'a EncodingRuntimeRegistry,
    /// Compiler-sealed tuple-encoding defaults used for tuple/static-table
    /// digests and execution witness encoding.
    pub tuple_encoding_defaults: &'a TupleEncodingDefaults,
    /// Installed canonical IR hash family implementation.
    pub hasher: &'a dyn Hasher,
}

/// Output of full native execution lowering.
#[derive(Debug, Clone)]
pub struct LoweringOutput {
    /// Instruction records for all opcodes across all successful txs.
    pub instruction_records: Vec<InstructionRecord>,
    /// Static table rows accumulated from lookup-like operations.
    pub static_table_rows: Vec<StaticTableRow>,
    /// Canonical IR-hash calls consumed by the dedicated hash lane.
    pub ir_hash_calls: Vec<IrHashCall>,
    /// Relation transcript calls consumed by the dedicated relation transcript lane.
    pub relation_transcript_calls: Vec<RelationTranscriptCall>,
    /// Relation claims aggregated across all successful txs.
    pub relation_claims: Vec<RelationClaim>,
}

/// Output of lowering one successful native transaction.
#[derive(Debug, Clone)]
pub struct TxLoweringOutput {
    /// Instruction records for all ops in the entry body.
    pub instruction_records: Vec<InstructionRecord>,
    /// Static table rows accumulated while lowering this entry.
    pub static_table_rows: Vec<StaticTableRow>,
    /// Canonical IR-hash calls consumed by the dedicated hash lane.
    pub ir_hash_calls: Vec<IrHashCall>,
    /// Relation transcript calls for this tx.
    pub relation_transcript_calls: Vec<RelationTranscriptCall>,
    /// Relation claims for this tx.
    pub relation_claims: Vec<RelationClaim>,
}

/// Merge per-tx lowering outputs into one execution-tier bundle.
pub fn merge_lowering_outputs<'a>(
    outputs: impl IntoIterator<Item = &'a TxLoweringOutput>,
) -> LoweringOutput {
    let mut instruction_records = Vec::new();
    let mut static_rows: BTreeMap<(u32, u16, u64), StaticTableRow> = BTreeMap::new();
    let mut ir_hash_calls = Vec::new();
    let mut relation_transcript_calls = Vec::new();
    let mut relation_claims = Vec::new();

    for output in outputs {
        instruction_records.extend(output.instruction_records.iter().cloned());
        ir_hash_calls.extend(output.ir_hash_calls.iter().cloned());
        relation_transcript_calls.extend(output.relation_transcript_calls.iter().cloned());
        relation_claims.extend(output.relation_claims.iter().cloned());
        for row in &output.static_table_rows {
            let key = (row.table_id, row.col_id, row.row_key);
            static_rows
                .entry(key)
                .and_modify(|existing| existing.lookup_mult += row.lookup_mult)
                .or_insert_with(|| row.clone());
        }
    }

    LoweringOutput {
        instruction_records,
        static_table_rows: static_rows.into_values().collect(),
        ir_hash_calls,
        relation_transcript_calls,
        relation_claims,
    }
}

/// Lower one successful native transaction into witness-ready execution records.
pub fn lower_successful_tx<const W: usize>(
    input: LowerSuccessfulTxInput<'_>,
) -> Result<TxLoweringOutput, TabulaError> {
    let mut lowering = LoweringCx::<W>::new(input)?;
    lowering.lower_entry()?;
    Ok(TxLoweringOutput {
        instruction_records: lowering.records,
        static_table_rows: Vec::new(),
        ir_hash_calls: lowering.ir_hash_calls,
        relation_transcript_calls: lowering.relation_transcript_calls,
        relation_claims: lowering.relation_claims,
    })
}
