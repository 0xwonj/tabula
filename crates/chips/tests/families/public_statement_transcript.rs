use p3_koala_bear::KoalaBear;

use tabula_chips::event_transcript::EventTranscriptChip;
use tabula_chips::public_context_transcript::PublicContextTranscriptChip;
use tabula_chips::tx_batch_transcript::TxBatchTranscriptChip;
use tabula_contract::format::public_statement_transcript::{
    EncodedTranscriptValue, PUBLIC_STATEMENT_TRANSCRIPT_RATE,
    compute_public_statement_transcript_digest, event_arg_block, event_header_block,
    event_transcript_header_block, public_context_header_block, public_context_item_block,
    tx_batch_header_block, tx_header_block, tx_param_block,
};
use tabula_core::TypeId;
use tabula_ir::{ContextFieldId, EntryId, EventId};
use tabula_stark::debug::debug_check_with_public_values;
use tabula_stark::trace::TraceGenerator;

fn digest_words(blocks: &[[KoalaBear; PUBLIC_STATEMENT_TRANSCRIPT_RATE]]) -> Vec<KoalaBear> {
    compute_public_statement_transcript_digest(blocks.iter())
        .0
        .to_vec()
}

fn encoded(type_id: u32, limbs: [u32; 3]) -> EncodedTranscriptValue {
    EncodedTranscriptValue {
        type_id: TypeId(type_id),
        field_elements: limbs.map(KoalaBear::new),
    }
}

#[test]
fn public_context_transcript_valid_empty_single_and_multi() {
    let chip = PublicContextTranscriptChip;

    let empty_items: Vec<[KoalaBear; PUBLIC_STATEMENT_TRANSCRIPT_RATE]> = vec![];
    let empty_trace = chip.generate_trace(empty_items.as_slice());
    let empty_blocks = vec![public_context_header_block(0)];
    debug_check_with_public_values(&chip, &empty_trace, &digest_words(&empty_blocks))
        .expect("empty public-context transcript should pass");

    let single_items = vec![public_context_item_block(
        ContextFieldId(7),
        &encoded(11, [101, 102, 103]),
    )];
    let single_trace = chip.generate_trace(single_items.as_slice());
    let single_blocks = vec![
        public_context_header_block(single_items.len()),
        single_items[0],
    ];
    debug_check_with_public_values(&chip, &single_trace, &digest_words(&single_blocks))
        .expect("single-item public-context transcript should pass");

    let multi_items = vec![
        public_context_item_block(ContextFieldId(7), &encoded(11, [101, 102, 103])),
        public_context_item_block(ContextFieldId(9), &encoded(12, [201, 202, 203])),
    ];
    let multi_trace = chip.generate_trace(multi_items.as_slice());
    let multi_blocks = vec![
        public_context_header_block(multi_items.len()),
        multi_items[0],
        multi_items[1],
    ];
    debug_check_with_public_values(&chip, &multi_trace, &digest_words(&multi_blocks))
        .expect("multi-item public-context transcript should pass");
}

#[test]
fn tx_batch_transcript_valid_empty_single_and_multi() {
    let chip = TxBatchTranscriptChip;

    let empty_items: Vec<[KoalaBear; PUBLIC_STATEMENT_TRANSCRIPT_RATE]> = vec![];
    let empty_trace = chip.generate_trace(empty_items.as_slice());
    let empty_blocks = vec![tx_batch_header_block(0)];
    debug_check_with_public_values(&chip, &empty_trace, &digest_words(&empty_blocks))
        .expect("empty tx-batch transcript should pass");

    let single_items = vec![
        tx_header_block(0, EntryId(5), 1),
        tx_param_block(0, 0, &encoded(31, [301, 302, 303])),
    ];
    let single_trace = chip.generate_trace(single_items.as_slice());
    let single_blocks = vec![tx_batch_header_block(1), single_items[0], single_items[1]];
    debug_check_with_public_values(&chip, &single_trace, &digest_words(&single_blocks))
        .expect("single-tx transcript should pass");

    let multi_items = vec![
        tx_header_block(0, EntryId(5), 1),
        tx_param_block(0, 0, &encoded(31, [301, 302, 303])),
        tx_header_block(1, EntryId(6), 2),
        tx_param_block(1, 0, &encoded(32, [401, 402, 403])),
        tx_param_block(1, 1, &encoded(33, [501, 502, 503])),
    ];
    let multi_trace = chip.generate_trace(multi_items.as_slice());
    let multi_blocks = vec![
        tx_batch_header_block(2),
        multi_items[0],
        multi_items[1],
        multi_items[2],
        multi_items[3],
        multi_items[4],
    ];
    debug_check_with_public_values(&chip, &multi_trace, &digest_words(&multi_blocks))
        .expect("multi-tx transcript should pass");
}

#[test]
fn event_transcript_valid_empty_single_and_multi() {
    let chip = EventTranscriptChip;

    let empty_items: Vec<[KoalaBear; PUBLIC_STATEMENT_TRANSCRIPT_RATE]> = vec![];
    let empty_trace = chip.generate_trace(empty_items.as_slice());
    let empty_blocks = vec![event_transcript_header_block(0)];
    debug_check_with_public_values(&chip, &empty_trace, &digest_words(&empty_blocks))
        .expect("empty event transcript should pass");

    let single_items = vec![
        event_header_block(0, 4, 0, EventId(17), 1),
        event_arg_block(0, 0, 0, &encoded(41, [601, 602, 603])),
    ];
    let single_trace = chip.generate_trace(single_items.as_slice());
    let single_blocks = vec![
        event_transcript_header_block(1),
        single_items[0],
        single_items[1],
    ];
    debug_check_with_public_values(&chip, &single_trace, &digest_words(&single_blocks))
        .expect("single-event transcript should pass");

    let multi_items = vec![
        event_header_block(0, 4, 0, EventId(17), 1),
        event_arg_block(0, 0, 0, &encoded(41, [601, 602, 603])),
        event_header_block(1, 9, 1, EventId(18), 2),
        event_arg_block(1, 1, 0, &encoded(42, [701, 702, 703])),
        event_arg_block(1, 1, 1, &encoded(43, [801, 802, 803])),
    ];
    let multi_trace = chip.generate_trace(multi_items.as_slice());
    let multi_blocks = vec![
        event_transcript_header_block(2),
        multi_items[0],
        multi_items[1],
        multi_items[2],
        multi_items[3],
        multi_items[4],
    ];
    debug_check_with_public_values(&chip, &multi_trace, &digest_words(&multi_blocks))
        .expect("multi-event transcript should pass");
}
