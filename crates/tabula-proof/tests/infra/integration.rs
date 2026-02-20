//! Multi-chip integration tests: end-to-end LogUp bus verification.
//!
//! These tests construct consistent traces across ALL chips simultaneously
//! and verify that all 8 LogUp buses balance in a single `check_logup_balance` call.

use std::collections::BTreeSet;

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_commitment::{ColumnMeta, CommitmentStrategy, NativeDigest};
use tabula_core::{ColId, TableId};

use tabula_proof::air::chips::column_meta::air::ColumnMetaChip;
use tabula_proof::air::chips::column_meta::trace::generate_column_meta_trace;
use tabula_proof::air::chips::execution::air::ExecutionChip;
use tabula_proof::air::chips::execution::trace::{
    InstructionRecord, Opcode, generate_execution_trace,
};
use tabula_proof::air::chips::merge::air::GlobalMergeChip;
use tabula_proof::air::chips::merge::trace::generate_merge_trace;
use tabula_proof::air::chips::poseidon::air::PoseidonChip;
use tabula_proof::air::chips::poseidon::constants::poseidon2_permutation;
use tabula_proof::air::chips::poseidon::trace::{
    generate_poseidon_preprocessed, generate_poseidon_trace,
};
use tabula_proof::air::chips::range_check::{
    RANGE_CHECK_SIZE, RangeCheckChip, generate_range_check_trace,
};
use tabula_proof::air::chips::sorted_mem::air::GlobalSortedMemChip;
use tabula_proof::air::chips::sorted_mem::trace::generate_sorted_mem_trace;
use tabula_proof::air::chips::ssmc::air::GlobalSsmcChip;
use tabula_proof::air::chips::ssmc::trace::generate_ssmc_trace;
use tabula_proof::air::debug::{
    check_logup_balance, evaluate_chip, evaluate_chip_with_preprocessed,
};
use tabula_proof::air::{MergeRow, MergeSource, SortedMemRow, SsmcEntry};

use crate::common::builders::{
    init_row, make_add, make_read, make_write, merge_val, read_row, write_row,
};

// ── Helpers (duplicated from bus.rs for isolation) ──

fn compose_ssmc_first_input(t: u32, c: u16, key: u64, val: [u32; 3]) -> [BabyBear; 16] {
    let mask_30: u64 = (1 << 30) - 1;
    let mut input = [BabyBear::ZERO; 16];
    input[0] = BabyBear::ZERO; // domain tag 0x00
    input[1] = BabyBear::new(t);
    input[2] = BabyBear::new(c as u32);
    input[3] = BabyBear::new((key & mask_30) as u32);
    input[4] = BabyBear::new(((key >> 30) & mask_30) as u32);
    input[5] = BabyBear::new((key >> 60) as u32);
    for (i, &v) in val.iter().enumerate() {
        input[6 + i] = BabyBear::new(v);
    }
    input
}

fn compose_merge_first_input(t: u32, c: u16, key: u64, new_val: [u32; 3]) -> [BabyBear; 16] {
    compose_ssmc_first_input(t, c, key, new_val)
}

fn poseidon_digest(input: [BabyBear; 16]) -> [BabyBear; 8] {
    let (_rounds, output) = poseidon2_permutation(input);
    core::array::from_fn(|j| output[j])
}

/// Add 4 range check sends for a u64's two 30-bit limbs (each half-decomposed).
/// limb2 (4-bit) is proven by Limb2Bits boolean decomposition, not RC sends.
fn add_u64_rc(mults: &mut [u32; RANGE_CHECK_SIZE], val: u64) {
    let mask_15: u64 = (1 << 15) - 1;
    let mask_30: u64 = (1 << 30) - 1;
    let l0 = (val & mask_30) as u32;
    let l1 = ((val >> 30) & mask_30) as u32;
    mults[(l0 & mask_15 as u32) as usize] += 1;
    mults[(l0 >> 15) as usize] += 1;
    mults[(l1 & mask_15 as u32) as usize] += 1;
    mults[(l1 >> 15) as usize] += 1;
}

/// Count range check multiplicities from ALL chips that send on the RangeCheck bus.
/// Each chip also sends 1 lex-direction range check per segment boundary.
fn count_range_check_multiplicities(
    sm_rows: &[SortedMemRow],
    ssmc_entries: &[SsmcEntry],
    merge_rows: &[MergeRow],
    exec_records: &[InstructionRecord],
    metas: &[ColumnMeta],
) -> [u32; RANGE_CHECK_SIZE] {
    let mut mults = [0u32; RANGE_CHECK_SIZE];

    // ── SortedMem: 12 sends per real row (+ lex) ──
    let sm_num_real = sm_rows.len();
    let sm_num_rows = (sm_num_real + 1).next_power_of_two().max(2);
    for i in 0..sm_num_real {
        let row = &sm_rows[i];
        add_u64_rc(&mut mults, row.row_key);
        add_u64_rc(&mut mults, row.timestamp);

        // Ordering diff: same logic as sorted_mem trace gen.
        let next_idx = (i + 1) % sm_num_rows;
        let gap = if next_idx < sm_num_real {
            let next = &sm_rows[next_idx];
            let tc_changed = row.table_id != next.table_id || row.col_id != next.col_id;
            let r_changed = tc_changed || row.row_key != next.row_key;
            if !r_changed {
                next.timestamp - row.timestamp - 1
            } else if !tc_changed {
                next.row_key - row.row_key - 1
            } else {
                0
            }
        } else {
            0
        };
        add_u64_rc(&mut mults, gap);

        // Lex ordering direction send (A2): 1 send per row with tc_changed=1.
        // tc_changed is derived from segment.populate(), wrapping to padding (0,0).
        let (sm_next_t, sm_next_c) = if next_idx < sm_num_real {
            (sm_rows[next_idx].table_id, sm_rows[next_idx].col_id as u32)
        } else {
            (0, 0)
        };
        let tc_val = row.table_id != sm_next_t || (row.col_id as u32) != sm_next_c;
        if tc_val {
            if next_idx >= sm_num_real {
                mults[0] += 1; // padding: lex columns=0, col_diff=0 sent
            } else if row.table_id != sm_next_t {
                mults[sm_next_t.wrapping_sub(row.table_id).wrapping_sub(1) as usize] += 1;
            } else {
                mults[sm_next_c.wrapping_sub(row.col_id as u32).wrapping_sub(1) as usize] += 1;
            }
        }
    }

    // ── SSMC: 8 sends per real entry (+ lex) ──
    let ssmc_num_real = ssmc_entries.len();
    let ssmc_num_rows = (ssmc_num_real + 1).next_power_of_two().max(2);
    for i in 0..ssmc_num_real {
        let entry = &ssmc_entries[i];
        add_u64_rc(&mut mults, entry.key);

        let next_idx = (i + 1) % ssmc_num_rows;
        let gap = if next_idx < ssmc_num_real {
            let next = &ssmc_entries[next_idx];
            let tc_changed = entry.table_id != next.table_id || entry.col_id != next.col_id;
            if !tc_changed {
                next.key - entry.key - 1
            } else {
                0
            }
        } else {
            0
        };
        add_u64_rc(&mut mults, gap);

        // SSMC lex send: tc_changed derived from segment.populate(), wrapping to padding (0,0).
        let (ssmc_next_t, ssmc_next_c) = if next_idx < ssmc_num_real {
            (
                ssmc_entries[next_idx].table_id,
                ssmc_entries[next_idx].col_id as u32,
            )
        } else {
            (0, 0)
        };
        let tc_val = entry.table_id != ssmc_next_t || (entry.col_id as u32) != ssmc_next_c;
        if tc_val {
            if next_idx >= ssmc_num_real {
                mults[0] += 1;
            } else if entry.table_id != ssmc_next_t {
                mults[ssmc_next_t.wrapping_sub(entry.table_id).wrapping_sub(1) as usize] += 1;
            } else {
                mults[ssmc_next_c
                    .wrapping_sub(entry.col_id as u32)
                    .wrapping_sub(1) as usize] += 1;
            }
        }
    }

    // ── Merge: 8 sends per real row (+ lex) ──
    let merge_num_real = merge_rows.len();
    let merge_num_rows = (merge_num_real + 1).next_power_of_two().max(2);
    for i in 0..merge_num_real {
        let row = &merge_rows[i];
        add_u64_rc(&mut mults, row.key);

        let next_idx = (i + 1) % merge_num_rows;
        let gap = if next_idx < merge_num_real {
            let next = &merge_rows[next_idx];
            let tc_changed = row.table_id != next.table_id || row.col_id != next.col_id;
            if !tc_changed {
                next.key - row.key - 1
            } else {
                0
            }
        } else {
            0
        };
        add_u64_rc(&mut mults, gap);

        // Merge lex send: tc_changed derived from segment.populate(), wrapping to padding (0,0).
        let (merge_next_t, merge_next_c) = if next_idx < merge_num_real {
            (
                merge_rows[next_idx].table_id,
                merge_rows[next_idx].col_id as u32,
            )
        } else {
            (0, 0)
        };
        let tc_val = row.table_id != merge_next_t || (row.col_id as u32) != merge_next_c;
        if tc_val {
            if next_idx >= merge_num_real {
                mults[0] += 1;
            } else if row.table_id != merge_next_t {
                mults[merge_next_t.wrapping_sub(row.table_id).wrapping_sub(1) as usize] += 1;
            } else {
                mults[merge_next_c.wrapping_sub(row.col_id as u32).wrapping_sub(1) as usize] += 1;
            }
        }
    }

    // ── Execution: 8 sends per access row ──
    let mut clk = 0u32;
    for rec in exec_records {
        let is_access = matches!(rec.opcode, Opcode::Read | Opcode::Write);
        if is_access {
            let tau = clk as u64 + 1;
            if let Some(r) = rec.access_r {
                add_u64_rc(&mut mults, r);
            }
            add_u64_rc(&mut mults, tau);
            clk += 1;
        }
    }

    // ── ColumnMeta: lex sends only ──
    // ColumnMeta sends lex RC for every real row with tc_changed=1.
    // tc_changed is derived from IsZero of table_diff and col_diff to next row (or padding).
    let cm_num_real = metas.len();
    let cm_num_rows = (cm_num_real + 1).next_power_of_two().max(2);
    for i in 0..cm_num_real {
        let meta = &metas[i];
        let next_idx = (i + 1) % cm_num_rows;
        let (next_t, next_c) = if next_idx < cm_num_real {
            (metas[next_idx].table.0, metas[next_idx].col.0 as u32)
        } else {
            (0, 0)
        };
        let tc_changed = meta.table.0 != next_t || (meta.col.0 as u32) != next_c;
        if tc_changed {
            if next_idx >= cm_num_real {
                // Padding: lex columns=0, sends col_diff=0
                mults[0] += 1;
            } else if meta.table.0 != next_t {
                mults[next_t.wrapping_sub(meta.table.0).wrapping_sub(1) as usize] += 1;
            } else {
                mults[next_c.wrapping_sub(meta.col.0 as u32).wrapping_sub(1) as usize] += 1;
            }
        }
    }

    mults
}

// ── Integration scenario ──
//
// State: table=0, col=0 has key=100 with value=42 (SSMC-committed).
//
// Transaction: Read(slot=0, key=100) → Add(slot=1, slot0+slot0) → Write(slot=1, key=100)
// Result: key=100 changes from 42 to 84.
//
// Chips involved:
//   Execution: 3 instructions (read, add, write) with operand linkage
//   SortedMem: init(r=100,v=42,τ=0) → read(τ=1) → write(τ=2,v=84)
//   SSMC: 1 entry (key=100, val=42) with hash chain, segment_is_touched=true
//   Merge: 1 both_row (old=42, write=84, new=84)
//   Poseidon: 2 permutations (1 for SSMC hash, 1 for Merge hash)
//   ColumnMeta: meta(t=0,c=0, com_old=ssmc_hash, com_new=merge_hash, touched=true)
//   RangeCheck: multiplicities from SortedMem r/tau limb decompositions

#[test]
fn integration_single_tx_all_buses_balanced() {
    // ── 1. Execution trace ──
    // Read key=100 → slot 0 (val=42), Add slot0+slot0 → slot 1 (val=84), Write slot 1 → key=100
    let exec_records = vec![
        make_read(0, 0, 0, 100, 42, false),
        make_add(1, 0, 0, 42, 42),
        make_write(1, 0, 0, 100, 84, false),
    ];
    let exec_trace = generate_execution_trace::<3>(&exec_records);

    // ── 2. SortedMem trace ──
    // init(τ=0) → read(τ=1) → write(τ=2, val=84)
    let sm_rows = vec![
        init_row(0, 0, 100, [42, 0, 0], false),
        read_row(0, 0, 100, 1, [42, 0, 0], false),
        write_row(0, 0, 100, 2, [84, 0, 0], false),
    ];
    let sm_trace = generate_sorted_mem_trace::<3>(&sm_rows);

    // ── 3. SSMC trace ──
    // Single entry for old state: key=100, val=42, segment_is_touched=true.
    let ssmc_perm_input = compose_ssmc_first_input(0, 0, 100, [42, 0, 0]);
    let ssmc_hash = poseidon_digest(ssmc_perm_input);

    let ssmc_entries = vec![SsmcEntry {
        table_id: 0,
        col_id: 0,
        key: 100,
        value: vec![BabyBear::new(42), BabyBear::ZERO, BabyBear::ZERO],
        hash_acc: ssmc_hash,
        mult_witness: true, // C2: receives membership proof from SortedMem init
        segment_is_touched: true, // C3: sends on MergeOldList
    }];
    let ssmc_trace = generate_ssmc_trace::<3>(&ssmc_entries);

    // ── 4. Merge trace ──
    // Both row: old=42, write=84, new=84.
    let merge_perm_input = compose_merge_first_input(0, 0, 100, [84, 0, 0]);
    let merge_hash = poseidon_digest(merge_perm_input);

    let merge_rows = vec![MergeRow {
        table_id: 0,
        col_id: 0,
        key: 100,
        source: MergeSource::Both,
        old_val: merge_val([42, 0, 0]),
        write_val: merge_val([84, 0, 0]),
        new_val: merge_val([84, 0, 0]),
        in_new: true,
        hash_acc: merge_hash,
    }];
    let merge_trace = generate_merge_trace::<3>(&merge_rows);

    // ── 5. Poseidon trace ──
    // 2 permutations: SSMC hash chain + Merge hash chain.
    let poseidon_inputs = vec![ssmc_perm_input, merge_perm_input];
    let poseidon_trace = generate_poseidon_trace(&poseidon_inputs);
    let poseidon_prep = generate_poseidon_preprocessed(poseidon_inputs.len());

    // ── 6. ColumnMeta trace ──
    let metas = vec![ColumnMeta {
        table: TableId(0),
        col: ColId(0),
        tag: CommitmentStrategy::Ssmc,
        com_old: NativeDigest(ssmc_hash),
        com_new: NativeDigest(merge_hash),
        is_empty_old: false,
        is_empty_new: false,
        is_touched: true,
    }];
    let sorted_mem_cols: BTreeSet<(u32, u16)> = [(0, 0)].into();
    let cm_trace = generate_column_meta_trace(&metas, &sorted_mem_cols);

    // ── 7. RangeCheck trace ──
    let rc_mults =
        count_range_check_multiplicities(&sm_rows, &ssmc_entries, &merge_rows, &exec_records, &metas);
    let rc_trace = generate_range_check_trace(&rc_mults);

    // ── Evaluate all chips ──
    let records = vec![
        evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap(),
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
        evaluate_chip("SSMC", &GlobalSsmcChip::<3>, &ssmc_trace).unwrap(),
        evaluate_chip("Merge", &GlobalMergeChip::<3>, &merge_trace).unwrap(),
        evaluate_chip_with_preprocessed(
            "Poseidon",
            &PoseidonChip,
            &poseidon_trace,
            Some(&poseidon_prep),
        )
        .unwrap(),
        evaluate_chip("ColumnMeta", &ColumnMetaChip, &cm_trace).unwrap(),
        evaluate_chip("RangeCheck", &RangeCheckChip, &rc_trace).unwrap(),
    ];

    // ── Verify ALL 8 buses balance simultaneously ──
    check_logup_balance(&records).expect("All 8 LogUp buses should balance");
}

#[test]
fn integration_corrupted_value_fails() {
    // Same scenario as above, but SortedMem write value is 85 instead of 84.
    // This should cause the Memory bus (C1) to be imbalanced.
    let exec_records = vec![
        make_read(0, 0, 0, 100, 42, false),
        make_add(1, 0, 0, 42, 42),
        make_write(1, 0, 0, 100, 84, false),
    ];
    let exec_trace = generate_execution_trace::<3>(&exec_records);

    // Wrong value: 85 instead of 84 in the write row.
    let sm_rows = vec![
        init_row(0, 0, 100, [42, 0, 0], false),
        read_row(0, 0, 100, 1, [42, 0, 0], false),
        write_row(0, 0, 100, 2, [85, 0, 0], false), // WRONG: 85 ≠ 84
    ];
    let sm_trace = generate_sorted_mem_trace::<3>(&sm_rows);

    let ssmc_perm_input = compose_ssmc_first_input(0, 0, 100, [42, 0, 0]);
    let ssmc_hash = poseidon_digest(ssmc_perm_input);
    let ssmc_entries = vec![SsmcEntry {
        table_id: 0,
        col_id: 0,
        key: 100,
        value: vec![BabyBear::new(42), BabyBear::ZERO, BabyBear::ZERO],
        hash_acc: ssmc_hash,
        mult_witness: true,
        segment_is_touched: true,
    }];
    let ssmc_trace = generate_ssmc_trace::<3>(&ssmc_entries);

    let merge_perm_input = compose_merge_first_input(0, 0, 100, [84, 0, 0]);
    let merge_hash = poseidon_digest(merge_perm_input);
    let merge_rows = vec![MergeRow {
        table_id: 0,
        col_id: 0,
        key: 100,
        source: MergeSource::Both,
        old_val: merge_val([42, 0, 0]),
        write_val: merge_val([84, 0, 0]),
        new_val: merge_val([84, 0, 0]),
        in_new: true,
        hash_acc: merge_hash,
    }];
    let merge_trace = generate_merge_trace::<3>(&merge_rows);

    let poseidon_inputs = vec![ssmc_perm_input, merge_perm_input];
    let poseidon_trace = generate_poseidon_trace(&poseidon_inputs);
    let poseidon_prep = generate_poseidon_preprocessed(poseidon_inputs.len());

    let metas = vec![ColumnMeta {
        table: TableId(0),
        col: ColId(0),
        tag: CommitmentStrategy::Ssmc,
        com_old: NativeDigest(ssmc_hash),
        com_new: NativeDigest(merge_hash),
        is_empty_old: false,
        is_empty_new: false,
        is_touched: true,
    }];
    let sorted_mem_cols: BTreeSet<(u32, u16)> = [(0, 0)].into();
    let cm_trace = generate_column_meta_trace(&metas, &sorted_mem_cols);

    let rc_mults =
        count_range_check_multiplicities(&sm_rows, &ssmc_entries, &merge_rows, &exec_records, &metas);
    let rc_trace = generate_range_check_trace(&rc_mults);

    let records = vec![
        evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap(),
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
        evaluate_chip("SSMC", &GlobalSsmcChip::<3>, &ssmc_trace).unwrap(),
        evaluate_chip("Merge", &GlobalMergeChip::<3>, &merge_trace).unwrap(),
        evaluate_chip_with_preprocessed(
            "Poseidon",
            &PoseidonChip,
            &poseidon_trace,
            Some(&poseidon_prep),
        )
        .unwrap(),
        evaluate_chip("ColumnMeta", &ColumnMetaChip, &cm_trace).unwrap(),
        evaluate_chip("RangeCheck", &RangeCheckChip, &rc_trace).unwrap(),
    ];

    check_logup_balance(&records).expect_err("Corrupted write value should cause LogUp imbalance");
}

#[test]
fn integration_missing_poseidon_perm_fails() {
    // Same scenario but only 1 Poseidon permutation instead of 2.
    // This should cause the PoseidonPermutation bus (C5) to be imbalanced.
    let exec_records = vec![
        make_read(0, 0, 0, 100, 42, false),
        make_add(1, 0, 0, 42, 42),
        make_write(1, 0, 0, 100, 84, false),
    ];
    let exec_trace = generate_execution_trace::<3>(&exec_records);

    let sm_rows = vec![
        init_row(0, 0, 100, [42, 0, 0], false),
        read_row(0, 0, 100, 1, [42, 0, 0], false),
        write_row(0, 0, 100, 2, [84, 0, 0], false),
    ];
    let sm_trace = generate_sorted_mem_trace::<3>(&sm_rows);

    let ssmc_perm_input = compose_ssmc_first_input(0, 0, 100, [42, 0, 0]);
    let ssmc_hash = poseidon_digest(ssmc_perm_input);
    let ssmc_entries = vec![SsmcEntry {
        table_id: 0,
        col_id: 0,
        key: 100,
        value: vec![BabyBear::new(42), BabyBear::ZERO, BabyBear::ZERO],
        hash_acc: ssmc_hash,
        mult_witness: true,
        segment_is_touched: true,
    }];
    let ssmc_trace = generate_ssmc_trace::<3>(&ssmc_entries);

    let merge_perm_input = compose_merge_first_input(0, 0, 100, [84, 0, 0]);
    let merge_hash = poseidon_digest(merge_perm_input);
    let merge_rows = vec![MergeRow {
        table_id: 0,
        col_id: 0,
        key: 100,
        source: MergeSource::Both,
        old_val: merge_val([42, 0, 0]),
        write_val: merge_val([84, 0, 0]),
        new_val: merge_val([84, 0, 0]),
        in_new: true,
        hash_acc: merge_hash,
    }];
    let merge_trace = generate_merge_trace::<3>(&merge_rows);

    // Only SSMC permutation — missing the Merge permutation.
    let poseidon_inputs = vec![ssmc_perm_input]; // MISSING merge_perm_input
    let poseidon_trace = generate_poseidon_trace(&poseidon_inputs);
    let poseidon_prep = generate_poseidon_preprocessed(poseidon_inputs.len());

    let metas = vec![ColumnMeta {
        table: TableId(0),
        col: ColId(0),
        tag: CommitmentStrategy::Ssmc,
        com_old: NativeDigest(ssmc_hash),
        com_new: NativeDigest(merge_hash),
        is_empty_old: false,
        is_empty_new: false,
        is_touched: true,
    }];
    let sorted_mem_cols: BTreeSet<(u32, u16)> = [(0, 0)].into();
    let cm_trace = generate_column_meta_trace(&metas, &sorted_mem_cols);

    let rc_mults =
        count_range_check_multiplicities(&sm_rows, &ssmc_entries, &merge_rows, &exec_records, &metas);
    let rc_trace = generate_range_check_trace(&rc_mults);

    let records = vec![
        evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap(),
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
        evaluate_chip("SSMC", &GlobalSsmcChip::<3>, &ssmc_trace).unwrap(),
        evaluate_chip("Merge", &GlobalMergeChip::<3>, &merge_trace).unwrap(),
        evaluate_chip_with_preprocessed(
            "Poseidon",
            &PoseidonChip,
            &poseidon_trace,
            Some(&poseidon_prep),
        )
        .unwrap(),
        evaluate_chip("ColumnMeta", &ColumnMetaChip, &cm_trace).unwrap(),
        evaluate_chip("RangeCheck", &RangeCheckChip, &rc_trace).unwrap(),
    ];

    check_logup_balance(&records)
        .expect_err("Missing Poseidon permutation should cause C5 imbalance");
}

// ── T9: Integration with multiple (t,c) segments ──
//
// State: (t=0,c=0) has key=10 with val=1; (t=0,c=1) has key=20 with val=2.
// Transaction: Read(key=10 from t=0,c=0) → Read(key=20 from t=0,c=1)
// No writes — both columns untouched. Tests lex ordering across segments.

#[test]
fn integration_multi_tc_all_buses_balanced() {
    // ── 1. Execution trace ──
    let exec_records = vec![
        make_read(0, 0, 0, 10, 1, false),  // Read from (t=0, c=0, r=10)
        make_read(1, 0, 1, 20, 2, false),  // Read from (t=0, c=1, r=20)
    ];
    let exec_trace = generate_execution_trace::<3>(&exec_records);

    // ── 2. SortedMem trace ──
    // Two segments: (t=0,c=0) and (t=0,c=1), sorted by (t,c,r,tau).
    let sm_rows = vec![
        init_row(0, 0, 10, [1, 0, 0], false),
        read_row(0, 0, 10, 1, [1, 0, 0], false),
        init_row(0, 1, 20, [2, 0, 0], false),
        read_row(0, 1, 20, 2, [2, 0, 0], false),
    ];
    let sm_trace = generate_sorted_mem_trace::<3>(&sm_rows);

    // ── 3. SSMC trace ──
    // Two segments: 1 entry each, neither touched (read-only).
    let ssmc_input_0 = compose_ssmc_first_input(0, 0, 10, [1, 0, 0]);
    let ssmc_hash_0 = poseidon_digest(ssmc_input_0);
    let ssmc_input_1 = compose_ssmc_first_input(0, 1, 20, [2, 0, 0]);
    let ssmc_hash_1 = poseidon_digest(ssmc_input_1);

    let ssmc_entries = vec![
        SsmcEntry {
            table_id: 0,
            col_id: 0,
            key: 10,
            value: vec![BabyBear::new(1), BabyBear::ZERO, BabyBear::ZERO],
            hash_acc: ssmc_hash_0,
            mult_witness: true,
            segment_is_touched: false,
        },
        SsmcEntry {
            table_id: 0,
            col_id: 1,
            key: 20,
            value: vec![BabyBear::new(2), BabyBear::ZERO, BabyBear::ZERO],
            hash_acc: ssmc_hash_1,
            mult_witness: true,
            segment_is_touched: false,
        },
    ];
    let ssmc_trace = generate_ssmc_trace::<3>(&ssmc_entries);

    // ── 4. Merge trace ──
    // No writes → no merge rows needed. Untouched columns.
    let merge_rows: Vec<MergeRow> = vec![];
    let merge_trace = generate_merge_trace::<3>(&merge_rows);

    // ── 5. Poseidon trace ──
    // 2 permutations for SSMC hash chains.
    let poseidon_inputs = vec![ssmc_input_0, ssmc_input_1];
    let poseidon_trace = generate_poseidon_trace(&poseidon_inputs);
    let poseidon_prep = generate_poseidon_preprocessed(poseidon_inputs.len());

    // ── 6. ColumnMeta trace ──
    let metas = vec![
        ColumnMeta {
            table: TableId(0),
            col: ColId(0),
            tag: CommitmentStrategy::Ssmc,
            com_old: NativeDigest(ssmc_hash_0),
            com_new: NativeDigest(ssmc_hash_0), // untouched → same
            is_empty_old: false,
            is_empty_new: false,
            is_touched: false,
        },
        ColumnMeta {
            table: TableId(0),
            col: ColId(1),
            tag: CommitmentStrategy::Ssmc,
            com_old: NativeDigest(ssmc_hash_1),
            com_new: NativeDigest(ssmc_hash_1),
            is_empty_old: false,
            is_empty_new: false,
            is_touched: false,
        },
    ];
    let sorted_mem_cols: BTreeSet<(u32, u16)> = [(0, 0), (0, 1)].into();
    let cm_trace = generate_column_meta_trace(&metas, &sorted_mem_cols);

    // ── 7. RangeCheck trace ──
    let rc_mults =
        count_range_check_multiplicities(&sm_rows, &ssmc_entries, &merge_rows, &exec_records, &metas);
    let rc_trace = generate_range_check_trace(&rc_mults);

    // ── Evaluate all chips ──
    let records = vec![
        evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap(),
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
        evaluate_chip("SSMC", &GlobalSsmcChip::<3>, &ssmc_trace).unwrap(),
        evaluate_chip("Merge", &GlobalMergeChip::<3>, &merge_trace).unwrap(),
        evaluate_chip_with_preprocessed(
            "Poseidon",
            &PoseidonChip,
            &poseidon_trace,
            Some(&poseidon_prep),
        )
        .unwrap(),
        evaluate_chip("ColumnMeta", &ColumnMetaChip, &cm_trace).unwrap(),
        evaluate_chip("RangeCheck", &RangeCheckChip, &rc_trace).unwrap(),
    ];

    // ── Verify ALL buses balance ──
    check_logup_balance(&records).expect("Multi-(t,c) integration should balance all buses");
}
