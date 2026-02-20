//! Multi-chip integration tests: end-to-end LogUp bus verification.
//!
//! Tests coordinated traces across Execution → InterTxOrder → StateColumn.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_proof::air::chips::execution::air::ExecutionChip;
use tabula_proof::air::chips::execution::trace::generate_execution_trace;
use tabula_proof::air::chips::inter_tx_order::air::InterTxOrderChip;
use tabula_proof::air::chips::inter_tx_order::trace::generate_inter_tx_order_trace;
use tabula_proof::air::chips::state_column::air::StateColumnChip;
use tabula_proof::air::chips::state_column::trace::{EntrySource, StateColumnRow};
use tabula_proof::air::chips::state_column::trace::generate_state_column_trace;
use tabula_proof::air::debug::{check_bus_balance, evaluate_chip};
use tabula_proof::air::interaction::InteractionKind;

use crate::common::builders::{
    ito_init, ito_read, ito_read_write, ito_write, make_read, make_write, sc_old_only,
};

fn bb_val(v: [u32; 3]) -> Vec<BabyBear> {
    v.iter().map(|x| BabyBear::new(*x)).collect()
}

/// Smoke test: a single Read instruction passes constraint check.
#[test]
fn single_read_constraints_pass() {
    let records = vec![make_read(0, 1, 0, 100, 42, false)];
    let exec_trace = generate_execution_trace::<3>(&records);
    let exec_chip = ExecutionChip::<3>;
    evaluate_chip("Execution", &exec_chip, &exec_trace)
        .expect("single Read should pass all constraints");
}

/// Conflicting batch: two txs both read+write the same key (echo writes).
///
/// tx_0: Read(key=100, val=50) → slot 0, Write(key=100, val=50) from slot 0.
/// tx_1: Read(key=100, val=50) → slot 0, Write(key=100, val=50) from slot 0.
///
/// Echo writes keep val unchanged — valid SSA with simple slot reuse.
/// Verifies C10, C11, C13, C14 bus balance across Exec → ITO → SC.
#[test]
fn conflicting_batch_full_chain() {
    // ── Execution trace ──
    // tx_0: Read(dst=0, key=100, val=50) → slot 0 = 50
    let mut r0 = make_read(0, 1, 0, 100, 50, false);
    r0.tx_index = 0;
    // tx_0: Write(src=0, key=100, val=50) — echo from slot 0
    let mut w0 = make_write(0, 1, 0, 100, 50, false);
    w0.tx_index = 0;
    // tx_1: Read(dst=0, key=100, val=50) → slot 0 = 50
    let mut r1 = make_read(0, 1, 0, 100, 50, false);
    r1.tx_index = 1;
    // tx_1: Write(src=0, key=100, val=50) — echo from slot 0
    let mut w1 = make_write(0, 1, 0, 100, 50, false);
    w1.tx_index = 1;
    let exec_trace = generate_execution_trace::<3>(&[r0, w0, r1, w1]);
    let exec_record = evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap();

    // ── InterTxOrder trace ──
    // init(base=50) → rw(tx0, 50→50) → rw(tx1, 50→50)
    let ito_rows = vec![
        ito_init(1, 0, 100, [50, 0, 0], false),
        ito_read_write(1, 0, 100, 0, [50, 0, 0], false, [50, 0, 0], false),
        ito_read_write(1, 0, 100, 1, [50, 0, 0], false, [50, 0, 0], false),
    ];
    let ito_trace = generate_inter_tx_order_trace::<3>(&ito_rows);
    let ito_record = evaluate_chip("InterTxOrder", &InterTxOrderChip::<3>, &ito_trace).unwrap();

    // ── StateColumn trace ──
    // "both" entry: old=50, new=50 (echo write, no value change)
    let sc_row = StateColumnRow {
        table_id: 1,
        col_id: 0,
        key: 100,
        is_gap: false,
        source: EntrySource::Both,
        old_val: bb_val([50, 0, 0]),
        new_val: bb_val([50, 0, 0]),
        segment_is_touched: true,
        old_hash_acc: [BabyBear::ZERO; 8],
        new_hash_acc: [BabyBear::ZERO; 8],
        read_mult: true,
        write_mult: true,
    };
    let sc_trace = generate_state_column_trace::<3>(&[sc_row]);
    let sc_record = evaluate_chip("StateColumn", &StateColumnChip::<3>, &sc_trace).unwrap();

    // ── Bus balance checks ──
    // C10 ReadAccess: Exec sends 2 reads → ITO receives 2
    check_bus_balance(
        &[exec_record.clone(), ito_record.clone()],
        InteractionKind::ReadAccess,
    )
    .expect("C10 ReadAccess should balance");

    // C11 WriteAccess: Exec sends 2 writes → ITO receives 2
    check_bus_balance(
        &[exec_record, ito_record.clone()],
        InteractionKind::WriteAccess,
    )
    .expect("C11 WriteAccess should balance");

    // C13 BaseStateEntry: ITO sends 1 init → SC receives 1
    check_bus_balance(
        &[ito_record.clone(), sc_record.clone()],
        InteractionKind::BaseStateEntry,
    )
    .expect("C13 BaseStateEntry should balance");

    // C14 CoalescedWrite: ITO sends 1 coalesced write (val=50) → SC receives 1
    check_bus_balance(&[ito_record, sc_record], InteractionKind::CoalescedWrite)
        .expect("C14 CoalescedWrite should balance");
}

/// Read-only batch: single tx reads a key without writing.
///
/// C10 balanced (Exec → ITO), C13 balanced (ITO → SC), no C11/C14 activity.
#[test]
fn read_only_chain() {
    // Execution: Read(key=100, val=42)
    let records = vec![make_read(0, 1, 0, 100, 42, false)];
    let exec_trace = generate_execution_trace::<3>(&records);
    let exec_record = evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap();

    // ITO: init(base=42) + read(tx=0)
    let ito_rows = vec![
        ito_init(1, 0, 100, [42, 0, 0], false),
        ito_read(1, 0, 100, 0, [42, 0, 0], false),
    ];
    let ito_trace = generate_inter_tx_order_trace::<3>(&ito_rows);
    let ito_record = evaluate_chip("InterTxOrder", &InterTxOrderChip::<3>, &ito_trace).unwrap();

    // SC: old_only entry with read_mult=true
    let mut sc_row = sc_old_only(1, 0, 100, [42, 0, 0]);
    sc_row.read_mult = true;
    let sc_trace = generate_state_column_trace::<3>(&[sc_row]);
    let sc_record = evaluate_chip("StateColumn", &StateColumnChip::<3>, &sc_trace).unwrap();

    // C10: Exec send → ITO receive
    check_bus_balance(
        &[exec_record, ito_record.clone()],
        InteractionKind::ReadAccess,
    )
    .expect("C10 should balance for read-only");

    // C13: ITO send → SC receive
    check_bus_balance(&[ito_record, sc_record], InteractionKind::BaseStateEntry)
        .expect("C13 should balance for read-only");
}

/// Multi-key: Read key=100 (val=42), Write that value to key=200.
///
/// Valid SSA: Read(dst=0, key=100) → Write(src=0, key=200).
/// Write value (42) comes from the read slot.
#[test]
fn multi_key_read_then_write() {
    // Read key=100 (val=42) into slot 0
    let r0 = make_read(0, 1, 0, 100, 42, false);
    // Write from slot 0 to key=200 (val=42)
    let w0 = make_write(0, 1, 0, 200, 42, false);
    let exec_trace = generate_execution_trace::<3>(&[r0, w0]);
    let exec_record = evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap();

    // ITO: two key chains in same (t=1, c=0) segment
    // key=100: init(42) + read(tx=0, input=42)
    // key=200: init(null) + write(tx=0, input=null, output=42)
    let ito_rows = vec![
        ito_init(1, 0, 100, [42, 0, 0], false),
        ito_read(1, 0, 100, 0, [42, 0, 0], false),
        ito_init(1, 0, 200, [0, 0, 0], true),
        ito_write(1, 0, 200, 0, [0, 0, 0], true, [42, 0, 0], false),
    ];
    let ito_trace = generate_inter_tx_order_trace::<3>(&ito_rows);
    let ito_record = evaluate_chip("InterTxOrder", &InterTxOrderChip::<3>, &ito_trace).unwrap();

    // C10: Read key=100
    check_bus_balance(
        &[exec_record.clone(), ito_record.clone()],
        InteractionKind::ReadAccess,
    )
    .expect("C10 should balance for multi-key read");

    // C11: Write key=200
    check_bus_balance(
        &[exec_record, ito_record.clone()],
        InteractionKind::WriteAccess,
    )
    .expect("C11 should balance for multi-key write");

    // SC: old_only(key=100) + write_only(key=200)
    let mut sc_old = sc_old_only(1, 0, 100, [42, 0, 0]);
    sc_old.segment_is_touched = true; // segment has a write (key=200)
    sc_old.read_mult = true;
    let sc_new = StateColumnRow {
        table_id: 1,
        col_id: 0,
        key: 200,
        is_gap: false,
        source: EntrySource::WriteOnly,
        old_val: vec![BabyBear::ZERO; 3],
        new_val: bb_val([42, 0, 0]),
        segment_is_touched: true,
        old_hash_acc: [BabyBear::ZERO; 8],
        new_hash_acc: [BabyBear::ZERO; 8],
        read_mult: true,  // receives C13 (null base state)
        write_mult: true,  // receives C14 (write)
    };
    let sc_trace = generate_state_column_trace::<3>(&[sc_old, sc_new]);
    let sc_record = evaluate_chip("StateColumn", &StateColumnChip::<3>, &sc_trace).unwrap();

    // C13: ITO sends 2 inits → SC receives 2 (old_only + write_only)
    check_bus_balance(
        &[ito_record.clone(), sc_record.clone()],
        InteractionKind::BaseStateEntry,
    )
    .expect("C13 should balance for multi-key");

    // C14: ITO sends 1 coalesced write (key=200, val=42) → SC receives 1
    check_bus_balance(&[ito_record, sc_record], InteractionKind::CoalescedWrite)
        .expect("C14 should balance for multi-key");
}
