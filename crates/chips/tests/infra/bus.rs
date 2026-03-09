//! Cross-chip LogUp bus integration tests.
//!
//! Tests that send/receive interactions balance across chip pairs.
//! Covers the 6-chip / 9-bus architecture:
//!
//! - C10 ReadAccess: Execution → InterTxOrder
//! - C11 WriteAccess: Execution → InterTxOrder
//! - C13 BaseStateEntry: InterTxOrder → StateColumn
//! - C14 CoalescedWrite: InterTxOrder → StateColumn
//! - C5  PoseidonPerm: ColumnMeta → Poseidon (tested below)
//! - C6  CommitmentVerif: StateColumn → ColumnMeta (tested below)
//! - C8  RangeCheck: sender chips → RangeCheck (tested below)
//! - C9  StaticTableLookup: Execution → StaticTable (tested below)
//! - C12 EmptyColRead: Execution → ColumnMeta (tested below)

use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_field::PrimeField32;

use tabula_commitment::{ColumnMeta, scheme_tags, NativeDigest};
use tabula_core::{ColId, TableId};

use tabula_chips::column_meta::air::ColumnMetaChip;
use tabula_chips::column_meta::trace::generate_column_meta_trace;
use tabula_chips::execution::air::ExecutionChip;
use tabula_chips::execution::trace::generate_execution_trace;
use tabula_chips::inter_tx_order::air::InterTxOrderChip;
use tabula_chips::inter_tx_order::trace::generate_inter_tx_order_trace;
use tabula_chips::poseidon::air::PoseidonChip;
use tabula_chips::poseidon::trace::{generate_poseidon_preprocessed, generate_poseidon_trace};
use tabula_chips::range_check::{RANGE_CHECK_SIZE, RangeCheckChip, generate_range_check_trace};
use tabula_chips::state_column::air::StateColumnChip;
use tabula_chips::state_column::trace::generate_state_column_trace;
use tabula_chips::static_table::air::StaticTableChip;
use tabula_chips::static_table::trace::{StaticTableRow, generate_static_table_trace};
use tabula_stark::air::interaction::{InteractionDirection, core_buses};
use tabula_stark::debug::{
    ChipRecord, check_bus_balance, evaluate_chip, evaluate_chip_with_preprocessed,
};

use tabula_chips::test_utils::builders::{
    ito_init, ito_read, ito_read_write, ito_write, make_lookup, make_read, make_write, sc_both,
    sc_old_only, sc_write_only,
};
use tabula_chips::test_utils::values::com_empty;

// ── C5 PoseidonPermutation ──

#[test]
fn c5_poseidon_com_empty_balance() {
    // ColumnMeta sends Com_empty verification → Poseidon receives.
    let com = com_empty(1, 0);

    let meta = ColumnMeta {
        table: TableId(1),
        col: ColId(0),
        tag: scheme_tags::SSMC,
        com_old: com,
        com_new: com,
        is_empty_old: true,
        is_empty_new: true,
        is_touched: false,
    };

    let cm_trace = generate_column_meta_trace(&[meta], &BTreeMap::new());
    let cm_chip = ColumnMetaChip;
    let cm_record = evaluate_chip("ColumnMeta", &cm_chip, &cm_trace).unwrap();

    // Poseidon: com_empty input = [0x00, table_id, col_id, 0..]
    let mut perm_input_empty = [BabyBear::ZERO; 16];
    perm_input_empty[1] = BabyBear::new(1); // table_id
    // perm_input_empty[2] = 0 (col_id)

    // Leaf digest inputs: [0x10, t, c, tag, 0,0,0,0, com[8]]
    // com = com_empty(1, 0) for both old and new (untouched empty column)
    let mut leaf_input = [BabyBear::ZERO; 16];
    leaf_input[0] = BabyBear::new(0x10);
    leaf_input[1] = BabyBear::new(1); // table_id
    // leaf_input[2] = 0 (col_id), leaf_input[3] = 0 (tag=SSMC)
    // leaf_input[8..16] = com_empty
    let com_fes = com.0;
    leaf_input[8..16].copy_from_slice(&com_fes);

    // ColumnMeta sends: 1 com_empty + 2 leaf digests (old + new, same since untouched)
    let pos_trace = generate_poseidon_trace(&[perm_input_empty, leaf_input, leaf_input]);
    let pos_chip = PoseidonChip;
    let pos_pre = generate_poseidon_preprocessed(3);
    let pos_record =
        evaluate_chip_with_preprocessed("Poseidon", &pos_chip, &pos_trace, Some(&pos_pre)).unwrap();

    check_bus_balance(&[cm_record, pos_record], core_buses::POSEIDON_PERM)
        .expect("C5 PoseidonPermutation bus should balance");
}

// ── C9 StaticTableLookup: Execution → StaticTable ──

#[test]
fn c9_static_table_lookup_balance_multiple_lookups() {
    // Two Lookup ops read the same static tuple.
    let mut l0 = make_lookup(0, 7, 0, 100, 42);
    l0.tx_index = 0;
    let mut l1 = make_lookup(1, 7, 0, 100, 42);
    l1.tx_index = 1;
    let exec_trace = generate_execution_trace::<3>(&[l0, l1]);
    let exec_record = evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap();

    // Static table row receives the same tuple with multiplicity 2.
    let static_rows = vec![StaticTableRow {
        table_id: 7,
        col_id: 0,
        row_key: 100,
        value: vec![BabyBear::new(42), BabyBear::ZERO, BabyBear::ZERO],
        lookup_mult: 2,
    }];
    let static_trace = generate_static_table_trace::<3>(&static_rows);
    let static_record = evaluate_chip("StaticTable", &StaticTableChip::<3>, &static_trace).unwrap();

    check_bus_balance(
        &[exec_record, static_record],
        core_buses::STATIC_TABLE_LOOKUP,
    )
    .expect("C9 StaticTableLookup should balance for duplicate lookups");
}

// ── C10 ReadAccess: Execution → InterTxOrder ──

#[test]
fn c10_read_access_single_read() {
    // Execution sends one Read → InterTxOrder receives it.
    let records = vec![make_read(0, 1, 0, 100, 42, false)];
    let exec_trace = generate_execution_trace::<3>(&records);
    let exec_record = evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap();

    // ITO: init(base=42) + read(tx=0, input=42)
    let ito_rows = vec![
        ito_init(1, 0, 100, [42, 0, 0], false),
        ito_read(1, 0, 100, 0, [42, 0, 0], false),
    ];
    let ito_trace = generate_inter_tx_order_trace::<3>(&ito_rows);
    let ito_record = evaluate_chip("InterTxOrder", &InterTxOrderChip::<3>, &ito_trace).unwrap();

    check_bus_balance(&[exec_record, ito_record], core_buses::READ_ACCESS)
        .expect("C10 ReadAccess bus should balance for single read");
}

#[test]
fn c10_read_access_two_reads_same_key() {
    // Two txs read the same key — Execution sends 2, ITO receives 2.
    let mut r0 = make_read(0, 1, 0, 100, 42, false);
    r0.tx_index = 0;
    let mut r1 = make_read(0, 1, 0, 100, 42, false);
    r1.tx_index = 1;
    let exec_trace = generate_execution_trace::<3>(&[r0, r1]);
    let exec_record = evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap();

    let ito_rows = vec![
        ito_init(1, 0, 100, [42, 0, 0], false),
        ito_read(1, 0, 100, 0, [42, 0, 0], false),
        ito_read(1, 0, 100, 1, [42, 0, 0], false),
    ];
    let ito_trace = generate_inter_tx_order_trace::<3>(&ito_rows);
    let ito_record = evaluate_chip("InterTxOrder", &InterTxOrderChip::<3>, &ito_trace).unwrap();

    check_bus_balance(&[exec_record, ito_record], core_buses::READ_ACCESS)
        .expect("C10 ReadAccess bus should balance for two reads");
}

#[test]
fn c10_read_access_tx_index_mismatch_fails() {
    // Same key/value but different tx_index must not balance.
    let mut r = make_read(0, 1, 0, 100, 42, false);
    r.tx_index = 0;
    let exec_trace = generate_execution_trace::<3>(&[r]);
    let exec_record = evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap();

    let ito_rows = vec![
        ito_init(1, 0, 100, [42, 0, 0], false),
        ito_read(1, 0, 100, 1, [42, 0, 0], false), // tx_index mismatch
    ];
    let ito_trace = generate_inter_tx_order_trace::<3>(&ito_rows);
    let ito_record = evaluate_chip("InterTxOrder", &InterTxOrderChip::<3>, &ito_trace).unwrap();

    check_bus_balance(&[exec_record, ito_record], core_buses::READ_ACCESS)
        .expect_err("C10 ReadAccess must include tx_index in tuple identity");
}

// ── C11 WriteAccess: Execution → InterTxOrder ──

#[test]
fn c11_write_access_single_write() {
    // Execution sends one Write → InterTxOrder receives it.
    // Valid SSA: Read(dst=0, key=100, val=75) populates slot 0, then Write(src=0, key=100, val=75).
    let records = vec![
        make_read(0, 1, 0, 100, 75, false),
        make_write(0, 1, 0, 100, 75, false),
    ];
    let exec_trace = generate_execution_trace::<3>(&records);
    let exec_record = evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap();

    // ITO: init(base=75) + read_write(tx=0, input=75, output=75) — echo write
    let ito_rows = vec![
        ito_init(1, 0, 100, [75, 0, 0], false),
        ito_read_write(1, 0, 100, 0, [75, 0, 0], false, [75, 0, 0], false),
    ];
    let ito_trace = generate_inter_tx_order_trace::<3>(&ito_rows);
    let ito_record = evaluate_chip("InterTxOrder", &InterTxOrderChip::<3>, &ito_trace).unwrap();

    check_bus_balance(&[exec_record, ito_record], core_buses::WRITE_ACCESS)
        .expect("C11 WriteAccess bus should balance for single write");
}

#[test]
fn c11_write_access_tx_index_mismatch_fails() {
    let mut r = make_read(0, 1, 0, 100, 75, false);
    r.tx_index = 0;
    let mut w = make_write(0, 1, 0, 100, 75, false);
    w.tx_index = 0;
    let exec_trace = generate_execution_trace::<3>(&[r, w]);
    let exec_record = evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap();

    let ito_rows = vec![
        ito_init(1, 0, 100, [75, 0, 0], false),
        ito_read_write(1, 0, 100, 1, [75, 0, 0], false, [75, 0, 0], false), // mismatch
    ];
    let ito_trace = generate_inter_tx_order_trace::<3>(&ito_rows);
    let ito_record = evaluate_chip("InterTxOrder", &InterTxOrderChip::<3>, &ito_trace).unwrap();

    check_bus_balance(&[exec_record, ito_record], core_buses::WRITE_ACCESS)
        .expect_err("C11 WriteAccess must include tx_index in tuple identity");
}

// ── C13 BaseStateEntry: InterTxOrder → StateColumn ──

#[test]
fn c13_base_state_entry_old_only() {
    // ITO sends init row → SC receives for old_only entry.
    let ito_rows = vec![
        ito_init(1, 0, 100, [42, 0, 0], false),
        ito_read(1, 0, 100, 0, [42, 0, 0], false),
    ];
    let ito_trace = generate_inter_tx_order_trace::<3>(&ito_rows);
    let ito_record = evaluate_chip("InterTxOrder", &InterTxOrderChip::<3>, &ito_trace).unwrap();

    // SC: old_only entry with read_mult=true to receive C13
    let mut sc_row = sc_old_only(1, 0, 100, [42, 0, 0]);
    sc_row.read_mult = true;
    let sc_trace = generate_state_column_trace::<3>(&[sc_row]);
    let sc_record = evaluate_chip("StateColumn", &StateColumnChip::<3>, &sc_trace).unwrap();

    check_bus_balance(&[ito_record, sc_record], core_buses::BASE_STATE_ENTRY)
        .expect("C13 BaseStateEntry should balance for old_only key");
}

#[test]
fn c13_base_state_entry_both_key() {
    // ITO sends init row for a key that is also written → SC "both" entry receives.
    let ito_rows = vec![
        ito_init(1, 0, 100, [50, 0, 0], false),
        ito_read_write(1, 0, 100, 0, [50, 0, 0], false, [75, 0, 0], false),
    ];
    let ito_trace = generate_inter_tx_order_trace::<3>(&ito_rows);
    let ito_record = evaluate_chip("InterTxOrder", &InterTxOrderChip::<3>, &ito_trace).unwrap();

    // SC: "both" entry with read_mult=true
    let mut sc_row = sc_both(1, 0, 100, [50, 0, 0], [75, 0, 0]);
    sc_row.read_mult = true;
    let sc_trace = generate_state_column_trace::<3>(&[sc_row]);
    let sc_record = evaluate_chip("StateColumn", &StateColumnChip::<3>, &sc_trace).unwrap();

    check_bus_balance(&[ito_record, sc_record], core_buses::BASE_STATE_ENTRY)
        .expect("C13 BaseStateEntry should balance for 'both' key");
}

#[test]
fn c13_base_state_entry_write_only() {
    // ITO sends init with null base → SC write_only entry receives.
    let ito_rows = vec![
        ito_init(1, 0, 100, [0, 0, 0], true),
        ito_write(1, 0, 100, 0, [0, 0, 0], true, [75, 0, 0], false),
    ];
    let ito_trace = generate_inter_tx_order_trace::<3>(&ito_rows);
    let ito_record = evaluate_chip("InterTxOrder", &InterTxOrderChip::<3>, &ito_trace).unwrap();

    // SC: write_only entry with read_mult=true
    let mut sc_row = sc_write_only(1, 0, 100, [75, 0, 0]);
    sc_row.read_mult = true;
    let sc_trace = generate_state_column_trace::<3>(&[sc_row]);
    let sc_record = evaluate_chip("StateColumn", &StateColumnChip::<3>, &sc_trace).unwrap();

    check_bus_balance(&[ito_record, sc_record], core_buses::BASE_STATE_ENTRY)
        .expect("C13 BaseStateEntry should balance for write_only key");
}

// ── C14 CoalescedWrite: InterTxOrder → StateColumn ──

#[test]
fn c14_coalesced_write_single() {
    // ITO sends coalesced write (last-for-key with write) → SC receives.
    let ito_rows = vec![
        ito_init(1, 0, 100, [50, 0, 0], false),
        ito_read_write(1, 0, 100, 0, [50, 0, 0], false, [75, 0, 0], false),
    ];
    let ito_trace = generate_inter_tx_order_trace::<3>(&ito_rows);
    let ito_record = evaluate_chip("InterTxOrder", &InterTxOrderChip::<3>, &ito_trace).unwrap();

    // SC: "both" entry (old=50, new=75) with write_mult=true
    let mut sc_row = sc_both(1, 0, 100, [50, 0, 0], [75, 0, 0]);
    sc_row.write_mult = true;
    let sc_trace = generate_state_column_trace::<3>(&[sc_row]);
    let sc_record = evaluate_chip("StateColumn", &StateColumnChip::<3>, &sc_trace).unwrap();

    check_bus_balance(&[ito_record, sc_record], core_buses::COALESCED_WRITE)
        .expect("C14 CoalescedWrite should balance");
}

#[test]
fn c14_coalesced_write_multi_tx_chain() {
    // tx0 writes 75, tx1 writes 90. Coalesced write = 90 (last writer's output).
    let ito_rows = vec![
        ito_init(1, 0, 100, [50, 0, 0], false),
        ito_read_write(1, 0, 100, 0, [50, 0, 0], false, [75, 0, 0], false),
        ito_read_write(1, 0, 100, 1, [75, 0, 0], false, [90, 0, 0], false),
    ];
    let ito_trace = generate_inter_tx_order_trace::<3>(&ito_rows);
    let ito_record = evaluate_chip("InterTxOrder", &InterTxOrderChip::<3>, &ito_trace).unwrap();

    // SC: "both" entry (old=50, new=90) with write_mult=true
    let mut sc_row = sc_both(1, 0, 100, [50, 0, 0], [90, 0, 0]);
    sc_row.write_mult = true;
    let sc_trace = generate_state_column_trace::<3>(&[sc_row]);
    let sc_record = evaluate_chip("StateColumn", &StateColumnChip::<3>, &sc_trace).unwrap();

    check_bus_balance(&[ito_record, sc_record], core_buses::COALESCED_WRITE)
        .expect("C14 CoalescedWrite should balance for multi-tx chain");
}

#[test]
fn c14_coalesced_write_delete() {
    // tx writes null (delete). Coalesced write has is_null=true.
    let ito_rows = vec![
        ito_init(1, 0, 100, [50, 0, 0], false),
        ito_read_write(1, 0, 100, 0, [50, 0, 0], false, [0, 0, 0], true),
    ];
    let ito_trace = generate_inter_tx_order_trace::<3>(&ito_rows);
    let ito_record = evaluate_chip("InterTxOrder", &InterTxOrderChip::<3>, &ito_trace).unwrap();

    // SC: "delete" entry with write_mult=true.
    // ITO sends (t, c, key, output_val=[0,0,0], output_is_null=true).
    // SC delete receive: (t, c, key, new_val=[0,0,0], is_delete=1).
    // These match because is_delete=1 maps to output_is_null=true.
    use tabula_chips::state_column::trace::EntrySource;
    use tabula_chips::state_column::trace::StateColumnRow;
    let sc_row = StateColumnRow {
        table_id: 1,
        col_id: 0,
        key: 100,
        is_gap: false,
        source: EntrySource::Delete,
        old_val: vec![BabyBear::new(50), BabyBear::ZERO, BabyBear::ZERO],
        new_val: vec![BabyBear::ZERO; 3],
        segment_is_touched: true,
        old_hash_acc: [BabyBear::ZERO; 8],
        new_hash_acc: [BabyBear::ZERO; 8],
        read_mult: false,
        write_mult: true,
    };
    let sc_trace = generate_state_column_trace::<3>(&[sc_row]);
    let sc_record = evaluate_chip("StateColumn", &StateColumnChip::<3>, &sc_trace).unwrap();

    check_bus_balance(&[ito_record, sc_record], core_buses::COALESCED_WRITE)
        .expect("C14 CoalescedWrite should balance for delete");
}

// ── C6 CommitmentVerification ──

#[test]
fn c6_commitment_verification_balances() {
    // StateColumn sends Com_old/Com_new at segment end.
    // ColumnMeta receives the same tuple identities.
    let com_old = digest_from_seed(100);
    let com_new = digest_from_seed(200);

    let mut sc_row = sc_both(1, 0, 100, [50, 0, 0], [75, 0, 0]);
    sc_row.old_hash_acc = com_old.0;
    sc_row.new_hash_acc = com_new.0;
    let sc_trace = generate_state_column_trace::<3>(&[sc_row]);
    let sc_record = evaluate_chip("StateColumn", &StateColumnChip::<3>, &sc_trace).unwrap();

    let meta = ColumnMeta {
        table: TableId(1),
        col: ColId(0),
        tag: scheme_tags::SSMC,
        com_old,
        com_new,
        is_empty_old: false,
        is_empty_new: false,
        is_touched: true,
    };
    let cm_trace = generate_column_meta_trace(&[meta], &BTreeMap::new());
    let cm_record = evaluate_chip("ColumnMeta", &ColumnMetaChip, &cm_trace).unwrap();

    check_bus_balance(&[sc_record, cm_record], core_buses::COMMITMENT_VERIF)
        .expect("C6 CommitmentVerification should balance");
}

#[test]
fn c6_commitment_verification_detects_digest_mismatch() {
    let com_old = digest_from_seed(100);
    let com_new = digest_from_seed(200);
    let wrong_com_new = digest_from_seed(201);

    let mut sc_row = sc_both(1, 0, 100, [50, 0, 0], [75, 0, 0]);
    sc_row.old_hash_acc = com_old.0;
    sc_row.new_hash_acc = com_new.0;
    let sc_trace = generate_state_column_trace::<3>(&[sc_row]);
    let sc_record = evaluate_chip("StateColumn", &StateColumnChip::<3>, &sc_trace).unwrap();

    let meta = ColumnMeta {
        table: TableId(1),
        col: ColId(0),
        tag: scheme_tags::SSMC,
        com_old,
        com_new: wrong_com_new,
        is_empty_old: false,
        is_empty_new: false,
        is_touched: true,
    };
    let cm_trace = generate_column_meta_trace(&[meta], &BTreeMap::new());
    let cm_record = evaluate_chip("ColumnMeta", &ColumnMetaChip, &cm_trace).unwrap();

    check_bus_balance(&[sc_record, cm_record], core_buses::COMMITMENT_VERIF)
        .expect_err("C6 must fail when Com_new digest mismatches");
}

// ── C8 RangeCheck ──

#[test]
fn c8_range_check_balances_aggregated_senders() {
    // Aggregate sends from multiple chips and synthesize RangeCheck multiplicities.
    let ito_rows = vec![
        ito_init(1, 0, 100, [50, 0, 0], false),
        ito_read_write(1, 0, 100, 0, [50, 0, 0], false, [75, 0, 0], false),
    ];
    let ito_trace = generate_inter_tx_order_trace::<3>(&ito_rows);
    let ito_record = evaluate_chip("InterTxOrder", &InterTxOrderChip::<3>, &ito_trace).unwrap();

    let mut sc_row = sc_both(1, 0, 100, [50, 0, 0], [75, 0, 0]);
    sc_row.old_hash_acc = digest_from_seed(300).0;
    sc_row.new_hash_acc = digest_from_seed(400).0;
    let sc_trace = generate_state_column_trace::<3>(&[sc_row]);
    let sc_record = evaluate_chip("StateColumn", &StateColumnChip::<3>, &sc_trace).unwrap();

    let senders = vec![ito_record.clone(), sc_record.clone()];
    let range_mults = collect_range_check_multiplicities(&senders);

    let rc_trace = generate_range_check_trace(&range_mults);
    let rc_record = evaluate_chip("RangeCheck", &RangeCheckChip, &rc_trace).unwrap();

    check_bus_balance(&[ito_record, sc_record, rc_record], core_buses::RANGE_CHECK)
        .expect("C8 RangeCheck should balance with synthesized multiplicities");
}

#[test]
fn c8_range_check_detects_multiplicity_mismatch() {
    let ito_rows = vec![
        ito_init(1, 0, 100, [50, 0, 0], false),
        ito_read(1, 0, 100, 0, [50, 0, 0], false),
    ];
    let ito_trace = generate_inter_tx_order_trace::<3>(&ito_rows);
    let ito_record = evaluate_chip("InterTxOrder", &InterTxOrderChip::<3>, &ito_trace).unwrap();

    let mut range_mults = collect_range_check_multiplicities(core::slice::from_ref(&ito_record));
    // Introduce one-off mismatch on a known sent value (100).
    range_mults[100] = range_mults[100].saturating_sub(1);

    let rc_trace = generate_range_check_trace(&range_mults);
    let rc_record = evaluate_chip("RangeCheck", &RangeCheckChip, &rc_trace).unwrap();

    check_bus_balance(&[ito_record, rc_record], core_buses::RANGE_CHECK)
        .expect_err("C8 must fail when multiplicity map is wrong");
}

fn digest_from_seed(seed: u32) -> NativeDigest {
    NativeDigest(core::array::from_fn(|i| BabyBear::new(seed + i as u32)))
}

// ── C12 EmptyColRead: Execution → ColumnMeta ──

#[test]
fn c12_empty_col_read_balance() {
    // Execution reads from an empty column → ColumnMeta receives (t, c).
    let mut r = make_read(0, 1, 0, 100, 0, true);
    r.is_empty_col = true;
    let exec_trace = generate_execution_trace::<3>(&[r]);
    let exec_record = evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap();

    // ColumnMeta: empty column with empty_read_mult=1.
    let com = com_empty(1, 0);
    let meta = ColumnMeta {
        table: TableId(1),
        col: ColId(0),
        tag: scheme_tags::SSMC,
        com_old: com,
        com_new: com,
        is_empty_old: true,
        is_empty_new: true,
        is_touched: false,
    };
    let mut empty_read_counts = BTreeMap::new();
    empty_read_counts.insert((1u32, 0u16), 1u32);
    let cm_trace = generate_column_meta_trace(&[meta], &empty_read_counts);
    let cm_record = evaluate_chip("ColumnMeta", &ColumnMetaChip, &cm_trace).unwrap();

    check_bus_balance(&[exec_record, cm_record], core_buses::EMPTY_COL_READ)
        .expect("C12 EmptyColRead bus should balance");
}

#[test]
fn c12_empty_col_read_multiple_reads() {
    // Two reads from the same empty column → multiplicity 2.
    let mut r0 = make_read(0, 2, 1, 10, 0, true);
    r0.is_empty_col = true;
    r0.tx_index = 0;
    let mut r1 = make_read(0, 2, 1, 20, 0, true);
    r1.is_empty_col = true;
    r1.tx_index = 1;
    let exec_trace = generate_execution_trace::<3>(&[r0, r1]);
    let exec_record = evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap();

    let com = com_empty(2, 1);
    let meta = ColumnMeta {
        table: TableId(2),
        col: ColId(1),
        tag: scheme_tags::SSMC,
        com_old: com,
        com_new: com,
        is_empty_old: true,
        is_empty_new: true,
        is_touched: false,
    };
    let mut empty_read_counts = BTreeMap::new();
    empty_read_counts.insert((2u32, 1u16), 2u32);
    let cm_trace = generate_column_meta_trace(&[meta], &empty_read_counts);
    let cm_record = evaluate_chip("ColumnMeta", &ColumnMetaChip, &cm_trace).unwrap();

    check_bus_balance(&[exec_record, cm_record], core_buses::EMPTY_COL_READ)
        .expect("C12 EmptyColRead bus should balance for multiple reads");
}

#[test]
fn c12_empty_col_read_multiplicity_mismatch_fails() {
    // Execution sends 1 empty-col read, but ColumnMeta has empty_read_mult=2 → imbalance.
    let mut r = make_read(0, 1, 0, 100, 0, true);
    r.is_empty_col = true;
    let exec_trace = generate_execution_trace::<3>(&[r]);
    let exec_record = evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap();

    let com = com_empty(1, 0);
    let meta = ColumnMeta {
        table: TableId(1),
        col: ColId(0),
        tag: scheme_tags::SSMC,
        com_old: com,
        com_new: com,
        is_empty_old: true,
        is_empty_new: true,
        is_touched: false,
    };
    let mut empty_read_counts = BTreeMap::new();
    empty_read_counts.insert((1u32, 0u16), 2u32); // Mismatch: 2 != 1
    let cm_trace = generate_column_meta_trace(&[meta], &empty_read_counts);
    let cm_record = evaluate_chip("ColumnMeta", &ColumnMetaChip, &cm_trace).unwrap();

    check_bus_balance(&[exec_record, cm_record], core_buses::EMPTY_COL_READ)
        .expect_err("C12 must fail when empty_read_mult mismatches actual sends");
}

fn collect_range_check_multiplicities(records: &[ChipRecord<BabyBear>]) -> [u32; RANGE_CHECK_SIZE] {
    let mut mults = [0u32; RANGE_CHECK_SIZE];

    for record in records {
        for i in &record.interactions {
            if i.bus != core_buses::RANGE_CHECK || i.direction != InteractionDirection::Send {
                continue;
            }
            if i.values.len() != 1 {
                continue;
            }
            let mult = i.multiplicity.as_canonical_u32();
            if mult == 0 {
                continue;
            }
            let value = i.values[0].as_canonical_u32() as usize;
            assert!(
                value < RANGE_CHECK_SIZE,
                "range-check send value out of domain: {value}"
            );
            mults[value] += mult;
        }
    }

    mults
}
