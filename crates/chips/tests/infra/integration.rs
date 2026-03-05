//! Multi-chip integration tests: end-to-end LogUp bus verification.
//!
//! Tests coordinated traces across Execution → InterTxOrder → StateColumn,
//! and ColumnMeta → SmtColPath → SmtTablePath.

use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_commitment::{ColumnMeta, CommitmentStrategy, NativeDigest};
use tabula_core::{ColId, TableId};

use tabula_chips::column_meta::air::ColumnMetaChip;
use tabula_chips::column_meta::trace::generate_column_meta_trace;
use tabula_chips::execution::air::ExecutionChip;
use tabula_chips::execution::trace::generate_execution_trace;
use tabula_chips::inter_tx_order::air::InterTxOrderChip;
use tabula_chips::inter_tx_order::trace::generate_inter_tx_order_trace;
use tabula_chips::poseidon::air::PoseidonChip;
use tabula_chips::poseidon::constants::poseidon2_permutation;
use tabula_chips::poseidon::trace::{generate_poseidon_preprocessed, generate_poseidon_trace};
use tabula_chips::public_input::check_public_input_binding;
use tabula_chips::smt_path::air::{SmtColPathChip, SmtTablePathChip};
use tabula_chips::smt_path::columns::SMT_TABLE_PATH_WIDTH;
use tabula_chips::smt_path::trace::{
    SmtPathWitness, SmtTablePathWitness, generate_smt_col_path_trace, generate_smt_table_path_trace,
};
use tabula_chips::state_column::air::StateColumnChip;
use tabula_chips::state_column::trace::generate_state_column_trace;
use tabula_chips::state_column::trace::{EntrySource, StateColumnRow};
use tabula_stark::air::interaction::core_buses;
use tabula_stark::debug::{
    check_bus_balance, evaluate_chip, evaluate_chip_with_preprocessed,
    evaluate_chip_with_public_values,
};

use tabula_chips::test_utils::builders::{
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
        core_buses::READ_ACCESS,
    )
    .expect("C10 ReadAccess should balance");

    // C11 WriteAccess: Exec sends 2 writes → ITO receives 2
    check_bus_balance(&[exec_record, ito_record.clone()], core_buses::WRITE_ACCESS)
        .expect("C11 WriteAccess should balance");

    // C13 BaseStateEntry: ITO sends 1 init → SC receives 1
    check_bus_balance(
        &[ito_record.clone(), sc_record.clone()],
        core_buses::BASE_STATE_ENTRY,
    )
    .expect("C13 BaseStateEntry should balance");

    // C14 CoalescedWrite: ITO sends 1 coalesced write (val=50) → SC receives 1
    check_bus_balance(&[ito_record, sc_record], core_buses::COALESCED_WRITE)
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
    check_bus_balance(&[exec_record, ito_record.clone()], core_buses::READ_ACCESS)
        .expect("C10 should balance for read-only");

    // C13: ITO send → SC receive
    check_bus_balance(&[ito_record, sc_record], core_buses::BASE_STATE_ENTRY)
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
        core_buses::READ_ACCESS,
    )
    .expect("C10 should balance for multi-key read");

    // C11: Write key=200
    check_bus_balance(&[exec_record, ito_record.clone()], core_buses::WRITE_ACCESS)
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
        write_mult: true, // receives C14 (write)
    };
    let sc_trace = generate_state_column_trace::<3>(&[sc_old, sc_new]);
    let sc_record = evaluate_chip("StateColumn", &StateColumnChip::<3>, &sc_trace).unwrap();

    // C13: ITO sends 2 inits → SC receives 2 (old_only + write_only)
    check_bus_balance(
        &[ito_record.clone(), sc_record.clone()],
        core_buses::BASE_STATE_ENTRY,
    )
    .expect("C13 should balance for multi-key");

    // C14: ITO sends 1 coalesced write (key=200, val=42) → SC receives 1
    check_bus_balance(&[ito_record, sc_record], core_buses::COALESCED_WRITE)
        .expect("C14 should balance for multi-key");
}

// ── SMT state root binding: ColumnMeta → SmtColPath → SmtTablePath ──

/// Helper: compute leaf digest via Poseidon permutation (matches ColumnMeta trace gen).
fn compute_leaf_digest(table: u32, col: u32, tag: u32, com: &NativeDigest) -> NativeDigest {
    let mut perm_input = [BabyBear::ZERO; 16];
    perm_input[0] = BabyBear::new(0x10); // DOMAIN_LEAF
    perm_input[1] = BabyBear::new(table);
    perm_input[2] = BabyBear::new(col);
    perm_input[3] = BabyBear::new(tag);
    perm_input[8..16].copy_from_slice(&com.0);
    let (_rounds, out) = poseidon2_permutation(perm_input);
    NativeDigest(core::array::from_fn(|j| out[j]))
}

/// Helper: compress two 8-FE digests via Poseidon permutation.
fn poseidon_compress(left: &NativeDigest, right: &NativeDigest) -> NativeDigest {
    let mut perm_input = [BabyBear::ZERO; 16];
    perm_input[..8].copy_from_slice(&left.0);
    perm_input[8..16].copy_from_slice(&right.0);
    let (_rounds, out) = poseidon2_permutation(perm_input);
    NativeDigest(core::array::from_fn(|j| out[j]))
}

/// End-to-end: ColumnMeta → SmtColPath → SmtTablePath with public input check.
///
/// Scenario: table 1 has 1 column (col 0) with different commitments
/// before and after the batch. Verifies:
/// - C15 balance (ColumnMeta → SmtColPath)
/// - C16 balance (SmtColPath → SmtTablePath)
/// - Public input binding (SmtTablePath root matches expected state root)
/// - C5 balance (all Poseidon permutations across ColumnMeta + both SMT chips)
#[test]
fn smt_state_root_end_to_end() {
    // ── Setup: single column (table 1, col 0) ──
    let com_old = NativeDigest(core::array::from_fn(|i| BabyBear::new(100 + i as u32)));
    let com_new = NativeDigest(core::array::from_fn(|i| BabyBear::new(200 + i as u32)));
    let old_leaf = compute_leaf_digest(1, 0, 0, &com_old);
    let new_leaf = compute_leaf_digest(1, 0, 0, &com_new);

    fn chain_compress(leaf: &NativeDigest, depth: usize) -> NativeDigest {
        let mut node = *leaf;
        for _ in 0..depth {
            node = poseidon_compress(&node, &NativeDigest::ZERO);
        }
        node
    }

    fn chain_compress_key1(leaf: &NativeDigest, depth: usize) -> NativeDigest {
        let mut node = *leaf;
        for level in 0..depth {
            if level == 0 {
                node = poseidon_compress(&NativeDigest::ZERO, &node);
            } else {
                node = poseidon_compress(&node, &NativeDigest::ZERO);
            }
        }
        node
    }

    // ── 1. ColumnMeta ──
    let metas = vec![ColumnMeta {
        table: TableId(1),
        col: ColId(0),
        tag: CommitmentStrategy::Ssmc,
        com_old,
        com_new,
        is_empty_old: false,
        is_empty_new: false,
        is_touched: true,
    }];
    let cm_trace = generate_column_meta_trace(&metas, &BTreeMap::new());
    let cm_record = evaluate_chip("ColumnMeta", &ColumnMetaChip, &cm_trace).unwrap();

    // ── 2. SmtColPath ──
    let col_path_witness = SmtPathWitness {
        table_id: 1,
        key: 0,
        old_leaf,
        new_leaf,
        siblings: zero_siblings(3),
        path_bits: vec![false, false, false],
    };
    let col_path_trace = generate_smt_col_path_trace(&[col_path_witness]);
    let col_path_record = evaluate_chip("SmtColPath", &SmtColPathChip, &col_path_trace).unwrap();

    // C15 balance: ColumnMeta → SmtColPath
    check_bus_balance(
        &[cm_record.clone(), col_path_record.clone()],
        core_buses::SMT_LEAF_DIGEST,
    )
    .expect("C15 SmtLeafDigest should balance");

    // ── 3. SmtTablePath ──
    let old_table_root = chain_compress(&old_leaf, 3);
    let new_table_root = chain_compress(&new_leaf, 3);

    let table_path = SmtTablePathWitness {
        path: SmtPathWitness {
            table_id: 1,
            key: 1, // table_id used as key
            old_leaf: old_table_root,
            new_leaf: new_table_root,
            siblings: zero_siblings(3),
            path_bits: vec![true, false, false],
        },
        root_mult: 1, // 1 column in this table
    };
    let table_path_trace = generate_smt_table_path_trace(&[table_path]);
    let old_state_root = chain_compress_key1(&old_table_root, 3);
    let new_state_root = chain_compress_key1(&new_table_root, 3);
    let mut pvs = Vec::with_capacity(16);
    pvs.extend_from_slice(&old_state_root.0);
    pvs.extend_from_slice(&new_state_root.0);
    let table_path_record = evaluate_chip_with_public_values(
        "SmtTablePath",
        &SmtTablePathChip,
        &table_path_trace,
        &pvs,
    )
    .unwrap();

    // C16 bus balance: SmtColPath → SmtTablePath
    check_bus_balance(
        &[col_path_record.clone(), table_path_record.clone()],
        core_buses::SMT_TABLE_ROOT,
    )
    .expect("C16 SmtTableRoot should balance");

    // ── 4. Public input binding ──
    check_public_input_binding(
        &table_path_trace,
        SMT_TABLE_PATH_WIDTH,
        &old_state_root.0,
        &new_state_root.0,
    )
    .expect("Public input binding should match SmtTablePath root rows");

    // ── 5. C5 PoseidonPerm balance ──
    // ColumnMeta: 2 perms (leaf old+new), SmtColPath: 6 (3 levels × 2), SmtTablePath: 6 = 14

    let mut all_perm_inputs = Vec::new();

    // ColumnMeta leaf perm inputs
    let mut leaf_input_old = [BabyBear::ZERO; 16];
    leaf_input_old[0] = BabyBear::new(0x10);
    leaf_input_old[1] = BabyBear::new(1);
    leaf_input_old[8..16].copy_from_slice(&com_old.0);
    all_perm_inputs.push(leaf_input_old);

    let mut leaf_input_new = [BabyBear::ZERO; 16];
    leaf_input_new[0] = BabyBear::new(0x10);
    leaf_input_new[1] = BabyBear::new(1);
    leaf_input_new[8..16].copy_from_slice(&com_new.0);
    all_perm_inputs.push(leaf_input_new);

    fn make_compress_input(node: &NativeDigest, sib: &NativeDigest, bit: bool) -> [BabyBear; 16] {
        let mut input = [BabyBear::ZERO; 16];
        let (left, right) = if bit { (sib, node) } else { (node, sib) };
        input[..8].copy_from_slice(&left.0);
        input[8..16].copy_from_slice(&right.0);
        input
    }

    // SmtColPath: key=0, bits=[false,false,false], zero siblings
    let zero = NativeDigest::ZERO;
    let inp = make_compress_input(&old_leaf, &zero, false);
    all_perm_inputs.push(inp);
    let level0_old = poseidon_compress(&old_leaf, &zero);
    let inp = make_compress_input(&new_leaf, &zero, false);
    all_perm_inputs.push(inp);
    let level0_new = poseidon_compress(&new_leaf, &zero);
    // Level 1 old
    let inp = make_compress_input(&level0_old, &zero, false);
    all_perm_inputs.push(inp);
    let level1_old = poseidon_compress(&level0_old, &zero);
    // Level 1 new
    let inp = make_compress_input(&level0_new, &zero, false);
    all_perm_inputs.push(inp);
    let level1_new = poseidon_compress(&level0_new, &zero);
    // Level 2 old
    let inp = make_compress_input(&level1_old, &zero, false);
    all_perm_inputs.push(inp);
    // Level 2 new
    let inp = make_compress_input(&level1_new, &zero, false);
    all_perm_inputs.push(inp);

    // SmtTablePath: key=1, bits=[true,false,false], zero siblings
    let inp = make_compress_input(&old_table_root, &zero, true);
    all_perm_inputs.push(inp);
    let tl0_old = poseidon_compress(&zero, &old_table_root);
    // Level 0 new
    let inp = make_compress_input(&new_table_root, &zero, true);
    all_perm_inputs.push(inp);
    let tl0_new = poseidon_compress(&zero, &new_table_root);
    // Level 1 old: bit=0
    let inp = make_compress_input(&tl0_old, &zero, false);
    all_perm_inputs.push(inp);
    let tl1_old = poseidon_compress(&tl0_old, &zero);
    // Level 1 new
    let inp = make_compress_input(&tl0_new, &zero, false);
    all_perm_inputs.push(inp);
    let tl1_new = poseidon_compress(&tl0_new, &zero);
    // Level 2 old
    let inp = make_compress_input(&tl1_old, &zero, false);
    all_perm_inputs.push(inp);
    // Level 2 new
    let inp = make_compress_input(&tl1_new, &zero, false);
    all_perm_inputs.push(inp);

    // Generate Poseidon trace with all inputs
    let pos_trace = generate_poseidon_trace(&all_perm_inputs);
    let pos_pre = generate_poseidon_preprocessed(all_perm_inputs.len());
    let pos_record =
        evaluate_chip_with_preprocessed("Poseidon", &PoseidonChip, &pos_trace, Some(&pos_pre))
            .unwrap();

    // C5 bus balance: all chips vs Poseidon
    check_bus_balance(
        &[cm_record, col_path_record, table_path_record, pos_record],
        core_buses::POSEIDON_PERM,
    )
    .expect("C5 PoseidonPermutation should balance across all SMT chips");
}

fn zero_siblings(depth: usize) -> Vec<NativeDigest> {
    vec![NativeDigest::ZERO; depth]
}
