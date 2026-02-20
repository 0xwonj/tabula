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
//! - C6  CommitmentVerif, C8 RangeCheck, C9 StaticTableLookup, C12 EmptyColRead: deferred

use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_commitment::{ColumnMeta, CommitmentStrategy};
use tabula_core::{ColId, TableId};

use tabula_proof::air::chips::column_meta::air::ColumnMetaChip;
use tabula_proof::air::chips::column_meta::trace::generate_column_meta_trace;
use tabula_proof::air::chips::execution::air::ExecutionChip;
use tabula_proof::air::chips::execution::trace::generate_execution_trace;
use tabula_proof::air::chips::inter_tx_order::air::InterTxOrderChip;
use tabula_proof::air::chips::inter_tx_order::trace::generate_inter_tx_order_trace;
use tabula_proof::air::chips::poseidon::air::PoseidonChip;
use tabula_proof::air::chips::poseidon::trace::{
    generate_poseidon_preprocessed, generate_poseidon_trace,
};
use tabula_proof::air::chips::state_column::air::StateColumnChip;
use tabula_proof::air::chips::state_column::trace::generate_state_column_trace;
use tabula_proof::air::debug::{check_bus_balance, evaluate_chip, evaluate_chip_with_preprocessed};
use tabula_proof::air::interaction::InteractionKind;

use crate::common::builders::{
    ito_init, ito_read, ito_read_write, ito_write, make_read, make_write, sc_both, sc_old_only,
    sc_write_only,
};
use crate::common::values::com_empty;

// ── C5 PoseidonPermutation ──

#[test]
fn c5_poseidon_com_empty_balance() {
    // ColumnMeta sends Com_empty verification → Poseidon receives.
    let com = com_empty(1, 0);

    let meta = ColumnMeta {
        table: TableId(1),
        col: ColId(0),
        tag: CommitmentStrategy::Ssmc,
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
    let mut perm_input = [BabyBear::ZERO; 16];
    perm_input[1] = BabyBear::new(1); // table_id
    // perm_input[2] = 0 (col_id)
    let pos_trace = generate_poseidon_trace(&[perm_input]);
    let pos_chip = PoseidonChip;
    let pos_pre = generate_poseidon_preprocessed(1);
    let pos_record =
        evaluate_chip_with_preprocessed("Poseidon", &pos_chip, &pos_trace, Some(&pos_pre)).unwrap();

    check_bus_balance(
        &[cm_record, pos_record],
        InteractionKind::PoseidonPermutation,
    )
    .expect("C5 PoseidonPermutation bus should balance");
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

    check_bus_balance(&[exec_record, ito_record], InteractionKind::ReadAccess)
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

    check_bus_balance(&[exec_record, ito_record], InteractionKind::ReadAccess)
        .expect("C10 ReadAccess bus should balance for two reads");
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

    check_bus_balance(&[exec_record, ito_record], InteractionKind::WriteAccess)
        .expect("C11 WriteAccess bus should balance for single write");
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

    check_bus_balance(&[ito_record, sc_record], InteractionKind::BaseStateEntry)
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

    check_bus_balance(&[ito_record, sc_record], InteractionKind::BaseStateEntry)
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

    check_bus_balance(&[ito_record, sc_record], InteractionKind::BaseStateEntry)
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

    check_bus_balance(&[ito_record, sc_record], InteractionKind::CoalescedWrite)
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

    check_bus_balance(&[ito_record, sc_record], InteractionKind::CoalescedWrite)
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
    use tabula_proof::air::chips::state_column::trace::EntrySource;
    use tabula_proof::air::chips::state_column::trace::StateColumnRow;
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

    check_bus_balance(&[ito_record, sc_record], InteractionKind::CoalescedWrite)
        .expect("C14 CoalescedWrite should balance for delete");
}

// ── C6 CommitmentVerification ──

#[test]
fn c6_commitment_verification_placeholder() {
    // StateColumn sends Com_old/Com_new → ColumnMeta receives.
    // Full integration test deferred (requires coordinated hash chains).
}

// ── C8 RangeCheck ──

#[test]
fn c8_range_check_placeholder() {
    // Multiple chips send half-decomposition values → RangeCheck receives.
    // Full integration test deferred (requires computing exact multiplicities).
}
