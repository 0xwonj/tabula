//! Canonical public-statement source-commitment transcript block schedules
//! shared by runtime and proof chips.

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_commitment::NativeDigest;
use tabula_core::TypeId;
use tabula_ir as ir;

use crate::format::static_tables::compute_block_chain_digest_from_iter;

/// Fixed block width used by public-statement transcript families.
pub const PUBLIC_STATEMENT_TRANSCRIPT_RATE: usize = 8;

/// Domain tag for the public-context transcript family.
pub const PUBLIC_CONTEXT_TRANSCRIPT_DOMAIN_TAG: u32 = 0x61;
/// Domain tag for the tx-batch transcript family.
pub const TX_BATCH_TRANSCRIPT_DOMAIN_TAG: u32 = 0x62;
/// Domain tag for the event transcript family.
pub const EVENT_TRANSCRIPT_DOMAIN_TAG: u32 = 0x63;

/// Fixed-width encoded typed value consumed by public-statement transcript
/// helpers.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedTranscriptValue {
    /// Semantic type id for the value.
    pub type_id: TypeId,
    /// Fixed execution-width field encoding.
    pub field_elements: [KoalaBear; 3],
}

/// Context transcript header block.
pub fn public_context_header_block(
    field_count: usize,
) -> [KoalaBear; PUBLIC_STATEMENT_TRANSCRIPT_RATE] {
    let mut block = [KoalaBear::ZERO; PUBLIC_STATEMENT_TRANSCRIPT_RATE];
    block[0] = KoalaBear::new(PUBLIC_CONTEXT_TRANSCRIPT_DOMAIN_TAG);
    block[1] = KoalaBear::new(field_count as u32);
    block
}

/// One public-context item block.
pub fn public_context_item_block(
    field_id: ir::ContextFieldId,
    value: &EncodedTranscriptValue,
) -> [KoalaBear; PUBLIC_STATEMENT_TRANSCRIPT_RATE] {
    let mut block = [KoalaBear::ZERO; PUBLIC_STATEMENT_TRANSCRIPT_RATE];
    block[0] = KoalaBear::new(field_id.0);
    block[1] = KoalaBear::new(value.type_id.0);
    block[2..5].copy_from_slice(&value.field_elements);
    block
}

/// Tx-batch transcript header block.
pub fn tx_batch_header_block(tx_count: usize) -> [KoalaBear; PUBLIC_STATEMENT_TRANSCRIPT_RATE] {
    let mut block = [KoalaBear::ZERO; PUBLIC_STATEMENT_TRANSCRIPT_RATE];
    block[0] = KoalaBear::new(TX_BATCH_TRANSCRIPT_DOMAIN_TAG);
    block[1] = KoalaBear::new(tx_count as u32);
    block
}

/// One per-transaction header block.
pub fn tx_header_block(
    tx_index: u32,
    entry_id: ir::EntryId,
    param_count: usize,
) -> [KoalaBear; PUBLIC_STATEMENT_TRANSCRIPT_RATE] {
    let mut block = [KoalaBear::ZERO; PUBLIC_STATEMENT_TRANSCRIPT_RATE];
    block[0] = KoalaBear::new(1);
    block[1] = KoalaBear::new(tx_index);
    block[2] = KoalaBear::new(entry_id.0);
    block[3] = KoalaBear::new(param_count as u32);
    block
}

/// One transaction parameter block.
pub fn tx_param_block(
    tx_index: u32,
    param_index: usize,
    value: &EncodedTranscriptValue,
) -> [KoalaBear; PUBLIC_STATEMENT_TRANSCRIPT_RATE] {
    let mut block = [KoalaBear::ZERO; PUBLIC_STATEMENT_TRANSCRIPT_RATE];
    block[0] = KoalaBear::new(2);
    block[1] = KoalaBear::new(tx_index);
    block[2] = KoalaBear::new(param_index as u32);
    block[3] = KoalaBear::new(value.type_id.0);
    block[4..7].copy_from_slice(&value.field_elements);
    block
}

/// Event transcript header block.
pub fn event_transcript_header_block(
    event_count: usize,
) -> [KoalaBear; PUBLIC_STATEMENT_TRANSCRIPT_RATE] {
    let mut block = [KoalaBear::ZERO; PUBLIC_STATEMENT_TRANSCRIPT_RATE];
    block[0] = KoalaBear::new(EVENT_TRANSCRIPT_DOMAIN_TAG);
    block[1] = KoalaBear::new(event_count as u32);
    block
}

/// One emitted-event header block.
pub fn event_header_block(
    tx_index: u32,
    instruction_index: usize,
    effect_ordinal_in_tx: u32,
    event_id: ir::EventId,
    arg_count: usize,
) -> [KoalaBear; PUBLIC_STATEMENT_TRANSCRIPT_RATE] {
    let mut block = [KoalaBear::ZERO; PUBLIC_STATEMENT_TRANSCRIPT_RATE];
    block[0] = KoalaBear::new(1);
    block[1] = KoalaBear::new(tx_index);
    block[2] = KoalaBear::new(instruction_index as u32);
    block[3] = KoalaBear::new(effect_ordinal_in_tx);
    block[4] = KoalaBear::new(event_id.0);
    block[5] = KoalaBear::new(arg_count as u32);
    block
}

/// One emitted-event argument block.
pub fn event_arg_block(
    tx_index: u32,
    effect_ordinal_in_tx: u32,
    arg_index: usize,
    value: &EncodedTranscriptValue,
) -> [KoalaBear; PUBLIC_STATEMENT_TRANSCRIPT_RATE] {
    let mut block = [KoalaBear::ZERO; PUBLIC_STATEMENT_TRANSCRIPT_RATE];
    block[0] = KoalaBear::new(2);
    block[1] = KoalaBear::new(tx_index);
    block[2] = KoalaBear::new(effect_ordinal_in_tx);
    block[3] = KoalaBear::new(arg_index as u32);
    block[4] = KoalaBear::new(value.type_id.0);
    block[5..8].copy_from_slice(&value.field_elements);
    block
}

/// Deterministic Poseidon block-chain digest over one canonical public-statement
/// source payload.
#[must_use]
pub fn compute_public_statement_transcript_digest<'a>(
    blocks: impl IntoIterator<Item = &'a [KoalaBear; PUBLIC_STATEMENT_TRANSCRIPT_RATE]>,
) -> NativeDigest {
    NativeDigest(compute_block_chain_digest_from_iter(blocks).map(KoalaBear::new))
}
