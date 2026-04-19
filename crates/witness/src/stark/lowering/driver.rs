//! Next-native witness lowering from canonical `tabula_ir` execution.
use std::collections::{BTreeMap, BTreeSet};

use p3_koala_bear::KoalaBear;

use tabula_chips::execution::trace::InstructionRecord;
use tabula_contract::format::typed_tuple::TupleEncodingDefaults;
use tabula_core::error::TabulaError;
use tabula_core::traits::Hasher;
use tabula_ir as ir;
use tabula_stark::witness_kit::KitScratch;
use tabula_types as exec;
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry, TypedValue};

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
    /// Proof-relevant property effects emitted by the executor.
    pub property_effects: &'a [exec::StatePropertyEffect],
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
    /// Installed user-state runtime view used by execution and proof lowering.
    pub state_runtime: &'a dyn exec::StateRuntimeView,
    /// Preloaded public-context slots reserved by the runtime-wide claim layout.
    pub context_slots: &'a [ContextPreludeSlot],
    /// Preloaded tx-parameter slots reserved by the runtime-wide claim layout.
    pub param_slots: &'a [ParamPreludeSlot],
    /// Exclusive upper bound for aux-slot allocation inside the tx body.
    pub aux_slot_limit: usize,
    /// Global event-transcript item index assigned to each active emit op.
    pub event_item_bases: &'a BTreeMap<usize, u32>,
}

/// One reserved execution slot preloaded with a canonical public-context value.
#[derive(Debug, Clone)]
pub struct ContextPreludeSlot {
    /// Context field identifier.
    pub field_id: ir::ContextFieldId,
    /// Reserved execution slot index.
    pub slot: usize,
    /// Typed value carried by the slot.
    pub value: TypedValue,
    /// Canonical execution-width encoding stored in the slot.
    pub encoded: Vec<KoalaBear>,
}

/// One reserved execution slot preloaded with a canonical tx-parameter value.
#[derive(Debug, Clone)]
pub struct ParamPreludeSlot {
    /// Entry parameter identifier.
    pub param_id: ir::ParamId,
    /// Reserved execution slot index.
    pub slot: usize,
    /// Typed value carried by the slot.
    pub value: TypedValue,
    /// Canonical execution-width encoding stored in the slot.
    pub encoded: Vec<KoalaBear>,
}

/// Output of full native execution lowering.
#[derive(Debug)]
pub struct LoweringOutput {
    /// Instruction records for all opcodes across all successful txs.
    pub instruction_records: Vec<InstructionRecord>,
    /// Relation claims aggregated across all successful txs.
    pub relation_claims: Vec<RelationClaim>,
    /// Per-chip opaque scratchpad populated via the
    /// [`ChipWitnessKit`](tabula_stark::witness_kit::ChipWitnessKit)
    /// authoring protocol. The execution-store assembly driver drains
    /// this map during `prepare_execution_store` by invoking each
    /// registered kit's `finalize`.
    pub kit_scratch: KitScratch,
}

/// Output of lowering one successful native transaction.
#[derive(Debug, Clone)]
pub struct TxLoweringOutput {
    /// Instruction records for all ops in the entry body.
    pub instruction_records: Vec<InstructionRecord>,
    /// Relation claims for this tx.
    pub relation_claims: Vec<RelationClaim>,
}

/// Merge per-tx lowering outputs into one execution-tier bundle.
///
/// The caller owns the shared [`KitScratch`] that opcode handlers
/// pushed rows into across tx calls and moves it into the merged
/// [`LoweringOutput`] here. Per-chip row order therefore matches the
/// emission order across the batch without a post-hoc merge step.
pub fn merge_lowering_outputs<'a>(
    outputs: impl IntoIterator<Item = &'a TxLoweringOutput>,
    kit_scratch: KitScratch,
) -> LoweringOutput {
    let mut instruction_records = Vec::new();
    let mut relation_claims = Vec::new();

    for output in outputs {
        instruction_records.extend(output.instruction_records.iter().cloned());
        relation_claims.extend(output.relation_claims.iter().cloned());
    }

    LoweringOutput {
        instruction_records,
        relation_claims,
        kit_scratch,
    }
}

/// Lower one successful native transaction into witness-ready execution records.
///
/// The shared `kit_scratch` is threaded through the lowering context so
/// chip kits can push rows inline as opcode handlers execute.
pub fn lower_successful_tx<const W: usize>(
    input: LowerSuccessfulTxInput<'_>,
    kit_scratch: &mut KitScratch,
) -> Result<TxLoweringOutput, TabulaError> {
    let mut lowering = LoweringCx::<W>::new(input, kit_scratch)?;
    lowering.lower_entry()?;
    Ok(TxLoweringOutput {
        instruction_records: lowering.records,
        relation_claims: lowering.relation_claims,
    })
}
