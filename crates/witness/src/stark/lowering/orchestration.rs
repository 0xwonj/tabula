//! Transaction-local lowering orchestration.
//!
//! Public entry points for lowering one successful execution shard to AIR trace
//! records.

use std::collections::{BTreeMap, BTreeSet};

use tabula_core::error::TabulaError;
use tabula_core::traits::StaticTableProvider;
use tabula_core::{ColId, TableId, Transaction};
use tabula_ir::{PrecompileId, PrecompileSignature, TxTypeDef};
use tabula_types::{
    EncodingRuntimeRegistry, TypeRuntimeRegistry, TypedPropertyQueryResult, TypedValue,
};

use tabula_chips::execution::trace::InstructionRecord;
use tabula_chips::ir_hash::IrHashCall;
use tabula_chips::static_table::trace::StaticTableRow;

use super::context::LoweringContext;
use super::lower_tx_body;
use crate::{AccessEvent, ColumnValueProfile};

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

/// Typed property-read result consumed by the lowering kernel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoweringPropertyRead {
    /// Instruction index within the tx body.
    pub instruction_index: usize,
    /// Typed property query result.
    pub result: TypedPropertyQueryResult,
}

/// Typed precompile call consumed by the lowering kernel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoweringPrecompileCall {
    /// Instruction index within the tx body.
    pub instruction_index: usize,
    /// Precompile identifier.
    pub precompile_id: PrecompileId,
    /// Typed input values.
    pub inputs: Vec<TypedValue>,
    /// Typed output values.
    pub outputs: Vec<TypedValue>,
}

/// Input bundle for lowering one successful transaction.
#[derive(Clone, Copy)]
pub struct LowerSuccessfulTxInput<'a> {
    /// Zero-based transaction index within the batch.
    pub tx_index: u32,
    /// Concrete transaction being lowered.
    pub tx: &'a Transaction,
    /// Resolved tx body being lowered.
    pub tx_def: &'a TxTypeDef,
    /// Precomputed column profile map for the program.
    pub profile_map: &'a BTreeMap<(TableId, ColId), ColumnValueProfile>,
    /// Installed type runtimes used for typed semantics.
    pub type_runtimes: &'a TypeRuntimeRegistry,
    /// Installed encoding runtimes used for witness encoding.
    pub encoding_runtimes: &'a EncodingRuntimeRegistry,
    /// Static table provider used by lookup lowering.
    pub static_tables: &'a dyn StaticTableProvider,
    /// Columns known to be empty in the old state.
    pub empty_columns: &'a BTreeSet<(TableId, ColId)>,
    /// Installed precompile signatures used by lowering.
    pub precompile_signatures: &'a BTreeMap<PrecompileId, PrecompileSignature>,
    /// Canonical access trace for this successful tx.
    pub access_trace: &'a [AccessEvent],
    /// Canonical precompile calls for this successful tx.
    pub precompile_calls: &'a [LoweringPrecompileCall],
    /// Canonical property-read results for this successful tx.
    pub property_reads: &'a [LoweringPropertyRead],
}

/// Output of lowering one successful transaction.
#[derive(Debug, Clone)]
pub struct TxLoweringOutput {
    /// Instruction records for all opcodes in the tx body.
    pub instruction_records: Vec<InstructionRecord>,
    /// Static table rows accumulated from Lookup instructions.
    pub static_table_rows: Vec<StaticTableRow>,
    /// Canonical IR-hash calls consumed by the dedicated hash lane.
    pub ir_hash_calls: Vec<IrHashCall>,
}

/// Lower one successful transaction into witness-ready trace records.
pub fn lower_successful_tx<const W: usize>(
    input: LowerSuccessfulTxInput<'_>,
) -> Result<TxLoweringOutput, TabulaError> {
    let tx_events = input.access_trace.iter().collect::<Vec<_>>();
    let mut ctx = LoweringContext::<W>::new(
        input.tx_index,
        &tx_events,
        input.profile_map,
        input.type_runtimes,
        input.encoding_runtimes,
        input.static_tables,
        input.empty_columns,
        input.tx.params.as_slice(),
        input.precompile_signatures,
        input.tx_def.body.len(),
        input.precompile_calls,
        input.property_reads,
    )?;

    lower_tx_body(&mut ctx, &input.tx_def.body)?;

    let (instruction_records, static_table_rows, ir_hash_calls) = ctx.into_output();
    Ok(TxLoweringOutput {
        instruction_records,
        static_table_rows,
        ir_hash_calls,
    })
}
