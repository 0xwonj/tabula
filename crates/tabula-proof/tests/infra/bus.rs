//! Cross-chip LogUp bus integration tests.
//!
//! Tests that send/receive interactions balance across chip pairs.
//!
//! Uses `check_bus_balance` for isolated bus testing (verifies a single bus
//! without requiring all other buses to be balanced).

use std::collections::BTreeSet;

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_commitment::{ColumnMeta, CommitmentStrategy, NativeDigest};
use tabula_core::{ColId, TableId};

use tabula_proof::air::chips::column_meta::air::ColumnMetaChip;
use tabula_proof::air::chips::column_meta::trace::generate_column_meta_trace;
use tabula_proof::air::chips::execution::air::ExecutionChip;
use tabula_proof::air::chips::execution::trace::generate_execution_trace;
use tabula_proof::air::chips::merge::air::GlobalMergeChip;
use tabula_proof::air::chips::merge::trace::generate_merge_trace;
use tabula_proof::air::chips::poseidon::air::PoseidonChip;
use tabula_proof::air::chips::poseidon::constants::poseidon2_permutation;
use tabula_proof::air::chips::poseidon::trace::generate_poseidon_trace;
use tabula_proof::air::chips::range_check::{
    RANGE_CHECK_SIZE, RangeCheckChip, generate_range_check_trace,
};
use tabula_proof::air::chips::sorted_mem::air::GlobalSortedMemChip;
use tabula_proof::air::chips::sorted_mem::trace::generate_sorted_mem_trace;
use tabula_proof::air::chips::ssmc::air::GlobalSsmcChip;
use tabula_proof::air::chips::ssmc::trace::generate_ssmc_trace;
use tabula_proof::air::debug::{check_bus_balance, evaluate_chip};
use tabula_proof::air::interaction::InteractionKind;

use tabula_proof::air::{MergeRow, MergeSource, SsmcEntry};

use crate::common::builders::{
    init_row, make_read, make_write, merge_val, merge_zeros, old_only_row, read_row, ssmc_entry,
    write_only_row, write_row,
};
use crate::common::values::{com_empty, distinct_digest};

// ── Helpers ──

/// Create a null init row (val_is_null=true, val=[0,0,0]).
fn null_init_row(t: u32, c: u16, r: u64) -> tabula_proof::air::SortedMemRow {
    init_row(t, c, r, [0, 0, 0], true)
}

/// Compose a 16-element Poseidon input for an SSMC first-entry hash chain step.
///
/// `[0x00, table_id, col_id, key[3], value[W], 0..]`
fn compose_ssmc_first_input(t: u32, c: u16, key: u64, val: [u32; 3]) -> [BabyBear; 16] {
    let mask_30: u64 = (1 << 30) - 1;
    let mut input = [BabyBear::ZERO; 16];
    input[0] = BabyBear::ZERO; // domain tag
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

/// Compose a 16-element Poseidon input for an SSMC continuation hash chain step.
///
/// `[prev_hash_acc[8], key[3], value[W], 0..]`
fn compose_ssmc_cont_input(
    prev_hash_acc: &[BabyBear; 8],
    key: u64,
    val: [u32; 3],
) -> [BabyBear; 16] {
    let mask_30: u64 = (1 << 30) - 1;
    let mut input = [BabyBear::ZERO; 16];
    input[..8].copy_from_slice(prev_hash_acc);
    input[8] = BabyBear::new((key & mask_30) as u32);
    input[9] = BabyBear::new(((key >> 30) & mask_30) as u32);
    input[10] = BabyBear::new((key >> 60) as u32);
    for (i, &v) in val.iter().enumerate() {
        input[11 + i] = BabyBear::new(v);
    }
    input
}

/// Compose a 16-element Poseidon input for a Merge first-in-new hash chain step.
fn compose_merge_first_input(t: u32, c: u16, key: u64, new_val: [u32; 3]) -> [BabyBear; 16] {
    // Same layout as SSMC first
    compose_ssmc_first_input(t, c, key, new_val)
}

/// Run Poseidon2 and return the first 8 elements of the output (digest).
fn poseidon_digest(input: [BabyBear; 16]) -> [BabyBear; 8] {
    let (_rounds, output) = poseidon2_permutation(input);
    core::array::from_fn(|j| output[j])
}

// ── C7: SortedMemMeta bus ──

#[test]
fn c7_sorted_mem_meta_balanced_single_segment() {
    let rows = vec![null_init_row(0, 0, 100)];
    let sm_trace = generate_sorted_mem_trace::<3>(&rows);

    let d1 = distinct_digest(1);
    let metas = vec![ColumnMeta {
        table: TableId(0),
        col: ColId(0),
        tag: CommitmentStrategy::Ssmc,
        com_old: d1,
        com_new: d1,
        is_empty_old: false,
        is_empty_new: false,
        is_touched: false,
    }];
    let sorted_mem_cols: BTreeSet<(u32, u16)> = [(0, 0)].into();
    let cm_trace = generate_column_meta_trace(&metas, &sorted_mem_cols);

    let records = vec![
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
        evaluate_chip("ColumnMeta", &ColumnMetaChip, &cm_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::SortedMemMeta)
        .expect("C7 SortedMemMeta should balance");
}

#[test]
fn c7_sorted_mem_meta_balanced_two_segments() {
    let rows = vec![null_init_row(0, 0, 100), null_init_row(0, 1, 200)];
    let sm_trace = generate_sorted_mem_trace::<3>(&rows);

    let d1 = distinct_digest(1);
    let d2 = distinct_digest(2);
    let metas = vec![
        ColumnMeta {
            table: TableId(0),
            col: ColId(0),
            tag: CommitmentStrategy::Ssmc,
            com_old: d1,
            com_new: d1,
            is_empty_old: false,
            is_empty_new: false,
            is_touched: false,
        },
        ColumnMeta {
            table: TableId(0),
            col: ColId(1),
            tag: CommitmentStrategy::Ssmc,
            com_old: d2,
            com_new: d2,
            is_empty_old: false,
            is_empty_new: false,
            is_touched: false,
        },
    ];
    let sorted_mem_cols: BTreeSet<(u32, u16)> = [(0, 0), (0, 1)].into();
    let cm_trace = generate_column_meta_trace(&metas, &sorted_mem_cols);

    let records = vec![
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
        evaluate_chip("ColumnMeta", &ColumnMetaChip, &cm_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::SortedMemMeta)
        .expect("C7 two segments should balance");
}

#[test]
fn c7_sorted_mem_meta_partial_columns() {
    let rows = vec![null_init_row(0, 0, 100)];
    let sm_trace = generate_sorted_mem_trace::<3>(&rows);

    let d1 = distinct_digest(1);
    let d2 = distinct_digest(2);
    let metas = vec![
        ColumnMeta {
            table: TableId(0),
            col: ColId(0),
            tag: CommitmentStrategy::Ssmc,
            com_old: d1,
            com_new: d1,
            is_empty_old: false,
            is_empty_new: false,
            is_touched: false,
        },
        ColumnMeta {
            table: TableId(0),
            col: ColId(1),
            tag: CommitmentStrategy::Ssmc,
            com_old: d2,
            com_new: d2,
            is_empty_old: false,
            is_empty_new: false,
            is_touched: false,
        },
    ];
    let sorted_mem_cols: BTreeSet<(u32, u16)> = [(0, 0)].into();
    let cm_trace = generate_column_meta_trace(&metas, &sorted_mem_cols);

    let records = vec![
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
        evaluate_chip("ColumnMeta", &ColumnMetaChip, &cm_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::SortedMemMeta)
        .expect("C7 partial columns should balance");
}

#[test]
fn c7_sorted_mem_meta_is_empty_old_matches() {
    let mut row = init_row(0, 0, 100, [1, 0, 0], false);
    row.meta_is_empty_old = true;
    let rows = vec![row];
    let sm_trace = generate_sorted_mem_trace::<3>(&rows);

    let metas = vec![ColumnMeta {
        table: TableId(0),
        col: ColId(0),
        tag: CommitmentStrategy::Ssmc,
        com_old: com_empty(0, 0),
        com_new: distinct_digest(1),
        is_empty_old: true,
        is_empty_new: false,
        is_touched: true,
    }];
    let sorted_mem_cols: BTreeSet<(u32, u16)> = [(0, 0)].into();
    let cm_trace = generate_column_meta_trace(&metas, &sorted_mem_cols);

    let records = vec![
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
        evaluate_chip("ColumnMeta", &ColumnMetaChip, &cm_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::SortedMemMeta)
        .expect("C7 is_empty_old=true should balance");
}

#[test]
fn c7_sorted_mem_meta_imbalanced_missing_receive() {
    let rows = vec![null_init_row(0, 0, 100)];
    let sm_trace = generate_sorted_mem_trace::<3>(&rows);

    let d1 = distinct_digest(1);
    let metas = vec![ColumnMeta {
        table: TableId(1),
        col: ColId(0),
        tag: CommitmentStrategy::Ssmc,
        com_old: d1,
        com_new: d1,
        is_empty_old: false,
        is_empty_new: false,
        is_touched: false,
    }];
    let sorted_mem_cols: BTreeSet<(u32, u16)> = BTreeSet::new();
    let cm_trace = generate_column_meta_trace(&metas, &sorted_mem_cols);

    let records = vec![
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
        evaluate_chip("ColumnMeta", &ColumnMetaChip, &cm_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::SortedMemMeta)
        .expect_err("C7 missing receive should fail");
}

// ── C2: SsmcMembership bus ──

#[test]
fn c2_ssmc_membership_balanced_single_key() {
    // SortedMem init (non-null, non-empty) → SSMC entry with mult_witness=true.
    let sm_rows = vec![init_row(0, 0, 100, [1, 2, 3], false)];
    let sm_trace = generate_sorted_mem_trace::<3>(&sm_rows);

    let mut entry = ssmc_entry(0, 0, 100, [1, 2, 3]);
    entry.mult_witness = true;
    let ssmc_trace = generate_ssmc_trace::<3>(&[entry]);

    let records = vec![
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
        evaluate_chip("SSMC", &GlobalSsmcChip::<3>, &ssmc_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::SsmcMembership)
        .expect("C2 single key should balance");
}

#[test]
fn c2_ssmc_membership_null_init_suppressed() {
    // Null init row (val_is_null=true) should NOT send on C2.
    let sm_rows = vec![null_init_row(0, 0, 100)];
    let sm_trace = generate_sorted_mem_trace::<3>(&sm_rows);

    let ssmc_trace = generate_ssmc_trace::<3>(&[]);

    let records = vec![
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
        evaluate_chip("SSMC", &GlobalSsmcChip::<3>, &ssmc_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::SsmcMembership)
        .expect("C2 null init should produce no sends");
}

#[test]
fn c2_ssmc_membership_empty_column_suppressed() {
    // meta_is_empty_old=true suppresses C2 send even with non-null val.
    let mut row = init_row(0, 0, 100, [1, 2, 3], false);
    row.meta_is_empty_old = true;
    let sm_rows = vec![row];
    let sm_trace = generate_sorted_mem_trace::<3>(&sm_rows);

    let ssmc_trace = generate_ssmc_trace::<3>(&[]);

    let records = vec![
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
        evaluate_chip("SSMC", &GlobalSsmcChip::<3>, &ssmc_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::SsmcMembership)
        .expect("C2 empty column suppresses send");
}

#[test]
fn c2_ssmc_membership_imbalanced_missing_receive() {
    // SortedMem sends on C2 but SSMC has mult_witness=0 → no receive.
    let sm_rows = vec![init_row(0, 0, 100, [1, 2, 3], false)];
    let sm_trace = generate_sorted_mem_trace::<3>(&sm_rows);

    let ssmc_entries = vec![ssmc_entry(0, 0, 100, [1, 2, 3])]; // mult_witness=false
    let ssmc_trace = generate_ssmc_trace::<3>(&ssmc_entries);

    let records = vec![
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
        evaluate_chip("SSMC", &GlobalSsmcChip::<3>, &ssmc_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::SsmcMembership)
        .expect_err("C2 missing receive should fail");
}

// ── C3: MergeOldList bus ──

#[test]
fn c3_merge_old_list_balanced_single_entry() {
    let mut entry = ssmc_entry(0, 0, 100, [1, 2, 3]);
    entry.segment_is_touched = true;
    let ssmc_trace = generate_ssmc_trace::<3>(&[entry]);

    let merge_rows = vec![old_only_row(0, 0, 100, [1, 2, 3])];
    let merge_trace = generate_merge_trace::<3>(&merge_rows);

    let records = vec![
        evaluate_chip("SSMC", &GlobalSsmcChip::<3>, &ssmc_trace).unwrap(),
        evaluate_chip("Merge", &GlobalMergeChip::<3>, &merge_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::MergeOldList)
        .expect("C3 single old_only should balance");
}

#[test]
fn c3_merge_old_list_untouched_suppressed() {
    let ssmc_entries = vec![ssmc_entry(0, 0, 100, [1, 2, 3])]; // touched=false
    let ssmc_trace = generate_ssmc_trace::<3>(&ssmc_entries);

    let merge_trace = generate_merge_trace::<3>(&[]);

    let records = vec![
        evaluate_chip("SSMC", &GlobalSsmcChip::<3>, &ssmc_trace).unwrap(),
        evaluate_chip("Merge", &GlobalMergeChip::<3>, &merge_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::MergeOldList)
        .expect("C3 untouched should produce no sends");
}

#[test]
fn c3_merge_old_list_multiple_old_only() {
    let mut e1 = ssmc_entry(0, 0, 10, [1, 0, 0]);
    e1.segment_is_touched = true;
    let mut e2 = ssmc_entry(0, 0, 20, [2, 0, 0]);
    e2.segment_is_touched = true;
    let ssmc_trace = generate_ssmc_trace::<3>(&[e1, e2]);

    let merge_rows = vec![
        old_only_row(0, 0, 10, [1, 0, 0]),
        old_only_row(0, 0, 20, [2, 0, 0]),
    ];
    let merge_trace = generate_merge_trace::<3>(&merge_rows);

    let records = vec![
        evaluate_chip("SSMC", &GlobalSsmcChip::<3>, &ssmc_trace).unwrap(),
        evaluate_chip("Merge", &GlobalMergeChip::<3>, &merge_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::MergeOldList)
        .expect("C3 multiple old_only should balance");
}

// ── C4: MergeWriteSet bus ──

#[test]
fn c4_merge_write_set_balanced_single_write() {
    let sm_rows = vec![
        null_init_row(0, 0, 100),
        write_row(0, 0, 100, 1, [4, 5, 6], false),
    ];
    let sm_trace = generate_sorted_mem_trace::<3>(&sm_rows);

    let merge_rows = vec![write_only_row(0, 0, 100, [4, 5, 6])];
    let merge_trace = generate_merge_trace::<3>(&merge_rows);

    let records = vec![
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
        evaluate_chip("Merge", &GlobalMergeChip::<3>, &merge_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::MergeWriteSet)
        .expect("C4 single write should balance");
}

#[test]
fn c4_merge_write_set_write_only() {
    let sm_rows = vec![
        null_init_row(0, 0, 200),
        write_row(0, 0, 200, 1, [7, 8, 9], false),
    ];
    let sm_trace = generate_sorted_mem_trace::<3>(&sm_rows);

    let merge_rows = vec![write_only_row(0, 0, 200, [7, 8, 9])];
    let merge_trace = generate_merge_trace::<3>(&merge_rows);

    let records = vec![
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
        evaluate_chip("Merge", &GlobalMergeChip::<3>, &merge_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::MergeWriteSet)
        .expect("C4 write_only should balance");
}

#[test]
fn c4_merge_write_set_read_only_no_send() {
    // Init only, no writes → has_written=0 → no C4 send.
    let sm_rows = vec![null_init_row(0, 0, 100)];
    let sm_trace = generate_sorted_mem_trace::<3>(&sm_rows);

    let merge_trace = generate_merge_trace::<3>(&[]);

    let records = vec![
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
        evaluate_chip("Merge", &GlobalMergeChip::<3>, &merge_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::MergeWriteSet)
        .expect("C4 read-only should not send");
}

// ── C6: CommitmentVerification bus ──

#[test]
fn c6_commitment_verification_balanced_com_old() {
    // SSMC segment → C6 send Com_old. ColumnMeta receives Com_old.
    let zero_digest = NativeDigest([BabyBear::ZERO; 8]);
    let mut entry = ssmc_entry(0, 0, 100, [1, 2, 3]);
    entry.segment_is_touched = false;
    // hash_acc defaults to [0;8], matching com_old below.
    let ssmc_trace = generate_ssmc_trace::<3>(&[entry]);

    let metas = vec![ColumnMeta {
        table: TableId(0),
        col: ColId(0),
        tag: CommitmentStrategy::Ssmc,
        com_old: zero_digest,
        com_new: zero_digest,
        is_empty_old: false,
        is_empty_new: false,
        is_touched: false,
    }];
    let cm_trace = generate_column_meta_trace(&metas, &Default::default());

    let records = vec![
        evaluate_chip("SSMC", &GlobalSsmcChip::<3>, &ssmc_trace).unwrap(),
        evaluate_chip("ColumnMeta", &ColumnMetaChip, &cm_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::CommitmentVerification)
        .expect("C6 Com_old should balance");
}

#[test]
fn c6_commitment_verification_balanced_com_new() {
    // Merge segment → C6 send Com_new. ColumnMeta receives Com_new.
    let zero_digest = NativeDigest([BabyBear::ZERO; 8]);

    let merge_rows = vec![write_only_row(0, 0, 100, [1, 2, 3])];
    let merge_trace = generate_merge_trace::<3>(&merge_rows);

    let metas = vec![ColumnMeta {
        table: TableId(0),
        col: ColId(0),
        tag: CommitmentStrategy::Ssmc,
        com_old: com_empty(0, 0), // Com_empty for is_empty_old=true
        com_new: zero_digest,     // hash_acc=[0;8] in Merge → matches com_new
        is_empty_old: true,       // suppresses Com_old receive (1-is_empty_old=0)
        is_empty_new: false,
        is_touched: true,
    }];
    let cm_trace = generate_column_meta_trace(&metas, &Default::default());

    let records = vec![
        evaluate_chip("Merge", &GlobalMergeChip::<3>, &merge_trace).unwrap(),
        evaluate_chip("ColumnMeta", &ColumnMetaChip, &cm_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::CommitmentVerification)
        .expect("C6 Com_new should balance");
}

#[test]
fn c6_commitment_verification_smt_suppressed() {
    // SMT tag (tag=1) suppresses C6 receive: (1-tag)=0.
    let ssmc_entries = vec![ssmc_entry(0, 0, 100, [1, 2, 3])];
    let ssmc_trace = generate_ssmc_trace::<3>(&ssmc_entries);

    let metas = vec![ColumnMeta {
        table: TableId(0),
        col: ColId(0),
        tag: CommitmentStrategy::Smt,
        com_old: distinct_digest(1),
        com_new: distinct_digest(2),
        is_empty_old: false,
        is_empty_new: false,
        is_touched: true,
    }];
    let cm_trace = generate_column_meta_trace(&metas, &Default::default());

    let records = vec![
        evaluate_chip("SSMC", &GlobalSsmcChip::<3>, &ssmc_trace).unwrap(),
        evaluate_chip("ColumnMeta", &ColumnMetaChip, &cm_trace).unwrap(),
    ];
    // SSMC sends on C6 but ColumnMeta tag=Smt suppresses receive → imbalanced.
    check_bus_balance(&records, InteractionKind::CommitmentVerification)
        .expect_err("C6 SMT column should not receive");
}

#[test]
fn c6_commitment_verification_empty_old_suppressed() {
    // is_empty_old=true suppresses C6 Com_old receive.
    let zero_digest = NativeDigest([BabyBear::ZERO; 8]);
    let metas = vec![ColumnMeta {
        table: TableId(0),
        col: ColId(0),
        tag: CommitmentStrategy::Ssmc,
        com_old: com_empty(0, 0), // Com_empty for is_empty_old=true
        com_new: zero_digest,     // hash_acc=[0;8] in Merge → matches com_new
        is_empty_old: true,
        is_empty_new: false,
        is_touched: true,
    }];
    let cm_trace = generate_column_meta_trace(&metas, &Default::default());

    // No SSMC (empty old → no SSMC data), but Merge sends Com_new.
    let merge_rows = vec![write_only_row(0, 0, 100, [1, 2, 3])];
    let merge_trace = generate_merge_trace::<3>(&merge_rows);

    let records = vec![
        evaluate_chip("Merge", &GlobalMergeChip::<3>, &merge_trace).unwrap(),
        evaluate_chip("ColumnMeta", &ColumnMetaChip, &cm_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::CommitmentVerification)
        .expect("C6 empty_old suppresses Com_old, Com_new balances");
}

// ── C1: Memory bus ──

#[test]
fn c1_memory_balanced_read() {
    // ExecutionChip sends read access, SortedMem receives it.
    let exec_records = vec![make_read(0, 0, 0, 100, 42, false)];
    let exec_trace = generate_execution_trace::<3>(&exec_records);

    // SortedMem: init row (not received on C1) + read row (received on C1).
    // The read row must match: t=0, c=0, r=100, tau=1, is_write=0, val=42, is_null=false.
    let sm_rows = vec![
        init_row(0, 0, 100, [42, 0, 0], false),
        read_row(0, 0, 100, 1, [42, 0, 0], false),
    ];
    let sm_trace = generate_sorted_mem_trace::<3>(&sm_rows);

    let records = vec![
        evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap(),
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::Memory).expect("C1 read should balance");
}

#[test]
fn c1_memory_balanced_write() {
    // ExecutionChip sends read + write access, SortedMem receives both.
    // Read 99 into slot 0, then write from slot 0.
    let exec_records = vec![
        make_read(0, 0, 0, 50, 99, false),
        make_write(0, 0, 0, 100, 99, false),
    ];
    let exec_trace = generate_execution_trace::<3>(&exec_records);

    // SortedMem: init for key 50 (read), read of key 50, init for key 100 (write), write of key 100.
    let sm_rows = vec![
        init_row(0, 0, 50, [99, 0, 0], false),
        read_row(0, 0, 50, 1, [99, 0, 0], false),
        init_row(0, 0, 100, [0, 0, 0], true),
        write_row(0, 0, 100, 2, [99, 0, 0], false),
    ];
    let sm_trace = generate_sorted_mem_trace::<3>(&sm_rows);

    let records = vec![
        evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap(),
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::Memory).expect("C1 write should balance");
}

#[test]
fn c1_memory_init_rows_suppressed() {
    // Init rows in SortedMem have mult=0 on C1 (1 - is_init = 0).
    // So an init-only SortedMem with no ExecutionChip should still balance at zero.
    let sm_rows = vec![null_init_row(0, 0, 100)];
    let sm_trace = generate_sorted_mem_trace::<3>(&sm_rows);

    let records = vec![evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap()];
    check_bus_balance(&records, InteractionKind::Memory)
        .expect("C1 init-only should have zero interactions");
}

#[test]
fn c1_memory_imbalanced_no_sorted_mem() {
    // ExecutionChip sends but no SortedMem → imbalanced.
    let exec_records = vec![make_read(0, 0, 0, 100, 42, false)];
    let exec_trace = generate_execution_trace::<3>(&exec_records);

    let records = vec![evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap()];
    check_bus_balance(&records, InteractionKind::Memory)
        .expect_err("C1 should be imbalanced without SortedMem");
}

// ── C8: RangeCheck bus ──

/// Count range-check multiplicities from SortedMem rows.
///
/// Each real row sends 12 values:
/// - 4 halves for r (limb2 proven by Limb2Bits)
/// - 4 halves for tau (limb2 proven by Limb2Bits)
/// - 4 halves for ordering diff (diff2 proven by Limb2Bits)
fn count_range_check_multiplicities(
    rows: &[tabula_proof::air::SortedMemRow],
) -> [u32; RANGE_CHECK_SIZE] {
    let mask_15: u64 = (1 << 15) - 1;
    let mask_30: u64 = (1 << 30) - 1;
    let num_real = rows.len();

    let mut mults = [0u32; RANGE_CHECK_SIZE];

    let add = |mults: &mut [u32; RANGE_CHECK_SIZE], val: u32| {
        assert!(
            (val as usize) < RANGE_CHECK_SIZE,
            "value {val} out of range"
        );
        mults[val as usize] += 1;
    };

    let num_rows = (num_real + 1).next_power_of_two().max(2);

    for i in 0..num_real {
        let row = &rows[i];
        let r = row.row_key;
        let r_l0 = (r & mask_30) as u32;
        let r_l1 = ((r >> 30) & mask_30) as u32;
        add(&mut mults, r_l0 & mask_15 as u32);
        add(&mut mults, r_l0 >> 15);
        add(&mut mults, r_l1 & mask_15 as u32);
        add(&mut mults, r_l1 >> 15);
        // r_l2 proven by Limb2Bits (4-bit boolean decomposition), no RC send

        let t = row.timestamp;
        let t_l0 = (t & mask_30) as u32;
        let t_l1 = ((t >> 30) & mask_30) as u32;
        add(&mut mults, t_l0 & mask_15 as u32);
        add(&mut mults, t_l0 >> 15);
        add(&mut mults, t_l1 & mask_15 as u32);
        add(&mut mults, t_l1 >> 15);
        // t_l2 proven by Limb2Bits (4-bit boolean decomposition), no RC send

        // Ordering diff halves (M10-A1): populated only for same-segment transitions.
        let next_idx = (i + 1) % num_rows;
        let mut gap = 0u64;
        if next_idx < num_real {
            let next = &rows[next_idx];
            let tc_changed = row.table_id != next.table_id || row.col_id != next.col_id;
            let r_changed = tc_changed || row.row_key != next.row_key;
            if !r_changed {
                gap = next.timestamp - row.timestamp - 1;
            } else if !tc_changed {
                gap = next.row_key - row.row_key - 1;
            }
        }
        let d0 = (gap & mask_30) as u32;
        let d1 = ((gap >> 30) & mask_30) as u32;
        add(&mut mults, d0 & mask_15 as u32);
        add(&mut mults, d0 >> 15);
        add(&mut mults, d1 & mask_15 as u32);
        add(&mut mults, d1 >> 15);
        // diff2 proven by Limb2Bits (4-bit boolean decomposition), no RC send

        // Lex ordering direction sends (A2): 1 send per row with tc_changed=1.
        // tc_changed is derived from segment.populate(), which wraps around:
        // for last real row, next is padding (t=0, c=0).
        // When next is padding, lex columns are zeros (populate not called),
        // so the RC send is value=0 with mult = is_real * tc_changed * (1-diff_is_table).
        let (next_t, next_c) = if next_idx < num_real {
            (rows[next_idx].table_id, rows[next_idx].col_id as u32)
        } else {
            (0, 0) // padding row
        };
        let tc_changed_val = row.table_id != next_t || (row.col_id as u32) != next_c;
        if tc_changed_val {
            if next_idx >= num_real {
                // Padding transition: lex columns unpopulated (zeros),
                // diff_is_table=0, so col_diff=0 sent with mult=1.
                add(&mut mults, 0);
            } else if row.table_id != next_t {
                add(
                    &mut mults,
                    next_t.wrapping_sub(row.table_id).wrapping_sub(1),
                );
            } else {
                add(
                    &mut mults,
                    next_c.wrapping_sub(row.col_id as u32).wrapping_sub(1),
                );
            }
        }
    }

    mults
}

#[test]
fn c8_range_check_balanced_simple() {
    // Single init row with small values: r=100, tau=0.
    let sm_rows = vec![null_init_row(0, 0, 100)];
    let sm_trace = generate_sorted_mem_trace::<3>(&sm_rows);

    let mults = count_range_check_multiplicities(&sm_rows);
    let rc_trace = generate_range_check_trace(&mults);

    let records = vec![
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
        evaluate_chip("RangeCheck", &RangeCheckChip, &rc_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::RangeCheck)
        .expect("C8 simple range check should balance");
}

#[test]
fn c8_range_check_balanced_with_access() {
    // Init + read row → 2 real rows, 20 range-check sends.
    let sm_rows = vec![
        init_row(0, 0, 100, [42, 0, 0], false),
        read_row(0, 0, 100, 1, [42, 0, 0], false),
    ];
    let sm_trace = generate_sorted_mem_trace::<3>(&sm_rows);

    let mults = count_range_check_multiplicities(&sm_rows);
    let rc_trace = generate_range_check_trace(&mults);

    let records = vec![
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
        evaluate_chip("RangeCheck", &RangeCheckChip, &rc_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::RangeCheck)
        .expect("C8 with access rows should balance");
}

#[test]
fn c8_range_check_imbalanced_wrong_multiplicities() {
    // Provide wrong multiplicities → should fail.
    let sm_rows = vec![null_init_row(0, 0, 100)];
    let sm_trace = generate_sorted_mem_trace::<3>(&sm_rows);

    let mults = [0u32; RANGE_CHECK_SIZE]; // all zeros → wrong
    let rc_trace = generate_range_check_trace(&mults);

    let records = vec![
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
        evaluate_chip("RangeCheck", &RangeCheckChip, &rc_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::RangeCheck)
        .expect_err("C8 should be imbalanced with zero multiplicities");
}

// ── C5: PoseidonPermutation bus ──

#[test]
fn c5_poseidon_perm_balanced_ssmc_single_entry() {
    // Single SSMC entry: compose perm_input, compute Poseidon digest → hash_acc.
    let perm_input = compose_ssmc_first_input(0, 0, 100, [1, 2, 3]);
    let hash_acc = poseidon_digest(perm_input);

    let entry = SsmcEntry {
        table_id: 0,
        col_id: 0,
        key: 100,
        value: vec![BabyBear::new(1), BabyBear::new(2), BabyBear::new(3)],
        hash_acc,
        mult_witness: false,
        segment_is_touched: false,
    };
    let ssmc_trace = generate_ssmc_trace::<3>(&[entry]);

    // Poseidon trace from the same input.
    let poseidon_trace = generate_poseidon_trace(&[perm_input]);

    let records = vec![
        evaluate_chip("SSMC", &GlobalSsmcChip::<3>, &ssmc_trace).unwrap(),
        evaluate_chip("Poseidon", &PoseidonChip, &poseidon_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::PoseidonPermutation)
        .expect("C5 SSMC single entry should balance");
}

#[test]
fn c5_poseidon_perm_balanced_ssmc_two_entries() {
    // Two SSMC entries in same segment: first + continuation hash chain.
    let input1 = compose_ssmc_first_input(0, 0, 10, [1, 0, 0]);
    let hash1 = poseidon_digest(input1);

    let input2 = compose_ssmc_cont_input(&hash1, 20, [2, 0, 0]);
    let hash2 = poseidon_digest(input2);

    let entries = vec![
        SsmcEntry {
            table_id: 0,
            col_id: 0,
            key: 10,
            value: vec![BabyBear::new(1), BabyBear::ZERO, BabyBear::ZERO],
            hash_acc: hash1,
            mult_witness: false,
            segment_is_touched: false,
        },
        SsmcEntry {
            table_id: 0,
            col_id: 0,
            key: 20,
            value: vec![BabyBear::new(2), BabyBear::ZERO, BabyBear::ZERO],
            hash_acc: hash2,
            mult_witness: false,
            segment_is_touched: false,
        },
    ];
    let ssmc_trace = generate_ssmc_trace::<3>(&entries);

    let poseidon_trace = generate_poseidon_trace(&[input1, input2]);

    let records = vec![
        evaluate_chip("SSMC", &GlobalSsmcChip::<3>, &ssmc_trace).unwrap(),
        evaluate_chip("Poseidon", &PoseidonChip, &poseidon_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::PoseidonPermutation)
        .expect("C5 SSMC two-entry chain should balance");
}

#[test]
fn c5_poseidon_perm_balanced_merge_single_entry() {
    // Single Merge row with in_new=1: compose, hash, verify C5 balance.
    let perm_input = compose_merge_first_input(0, 0, 100, [4, 5, 6]);
    let hash_acc = poseidon_digest(perm_input);

    let merge_rows = vec![MergeRow {
        table_id: 0,
        col_id: 0,
        key: 100,
        source: MergeSource::WriteOnly,
        old_val: merge_zeros(),
        write_val: merge_val([4, 5, 6]),
        new_val: merge_val([4, 5, 6]),
        in_new: true,
        hash_acc,
    }];
    let merge_trace = generate_merge_trace::<3>(&merge_rows);

    let poseidon_trace = generate_poseidon_trace(&[perm_input]);

    let records = vec![
        evaluate_chip("Merge", &GlobalMergeChip::<3>, &merge_trace).unwrap(),
        evaluate_chip("Poseidon", &PoseidonChip, &poseidon_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::PoseidonPermutation)
        .expect("C5 Merge single entry should balance");
}

#[test]
fn c5_poseidon_perm_merge_delete_suppressed() {
    // Merge row with in_new=0 (delete) should NOT send on C5.
    // Only the in_new=1 row participates.
    let first_input = compose_merge_first_input(0, 0, 10, [1, 0, 0]);
    let hash1 = poseidon_digest(first_input);

    let merge_rows = vec![
        // Delete row (in_new=0): carries hash_acc forward but doesn't hash.
        MergeRow {
            table_id: 0,
            col_id: 0,
            key: 5,
            source: MergeSource::Delete,
            old_val: merge_val([9, 0, 0]),
            write_val: merge_zeros(),
            new_val: merge_zeros(),
            in_new: false,
            hash_acc: [BabyBear::ZERO; 8], // carry forward (no prev → zero)
        },
        // Write-only row (in_new=1): first_in_new=true.
        MergeRow {
            table_id: 0,
            col_id: 0,
            key: 10,
            source: MergeSource::WriteOnly,
            old_val: merge_zeros(),
            write_val: merge_val([1, 0, 0]),
            new_val: merge_val([1, 0, 0]),
            in_new: true,
            hash_acc: hash1,
        },
    ];
    let merge_trace = generate_merge_trace::<3>(&merge_rows);

    // Only one Poseidon call (from the in_new=1 row).
    let poseidon_trace = generate_poseidon_trace(&[first_input]);

    let records = vec![
        evaluate_chip("Merge", &GlobalMergeChip::<3>, &merge_trace).unwrap(),
        evaluate_chip("Poseidon", &PoseidonChip, &poseidon_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::PoseidonPermutation)
        .expect("C5 Merge delete should not send on C5");
}

#[test]
fn c5_poseidon_perm_imbalanced_wrong_hash() {
    // SSMC entry with wrong hash_acc → C5 should be imbalanced.
    let perm_input = compose_ssmc_first_input(0, 0, 100, [1, 2, 3]);
    let wrong_hash = [BabyBear::new(999); 8]; // incorrect

    let entry = SsmcEntry {
        table_id: 0,
        col_id: 0,
        key: 100,
        value: vec![BabyBear::new(1), BabyBear::new(2), BabyBear::new(3)],
        hash_acc: wrong_hash,
        mult_witness: false,
        segment_is_touched: false,
    };
    let ssmc_trace = generate_ssmc_trace::<3>(&[entry]);

    let poseidon_trace = generate_poseidon_trace(&[perm_input]);

    let records = vec![
        evaluate_chip("SSMC", &GlobalSsmcChip::<3>, &ssmc_trace).unwrap(),
        evaluate_chip("Poseidon", &PoseidonChip, &poseidon_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::PoseidonPermutation)
        .expect_err("C5 wrong hash should be imbalanced");
}

// ── C5: PoseidonPermutation bus (Execution Hash opcode) ──

#[test]
fn c5_poseidon_perm_balanced_execution_hash() {
    // Execution Hash opcode sends on C5, Poseidon chip receives.
    // Reads put u64_to_limbs(1)=[1,0,0] into slot 0 and u64_to_limbs(4)=[4,0,0] into slot 1.
    // Hash src1/src2 must match these slot values for operand linkage to pass.
    let hash_rec = crate::common::builders::make_hash(
        2, // dst_slot
        0, // src1_slot
        1, // src2_slot
        0x20, 2, // 0x20 = HASH_INSTRUCTION_DOMAIN_TAG
        [1, 0, 0], [4, 0, 0],
    );
    let perm_input = hash_rec.hash_perm_input.unwrap();

    let exec_records = vec![
        make_read(0, 0, 0, 100, 1, false), // seeds slot 0 with [1,0,0]
        make_read(1, 0, 0, 200, 4, false), // seeds slot 1 with [4,0,0]
        hash_rec,
    ];
    let exec_trace = generate_execution_trace::<3>(&exec_records);

    let poseidon_trace = generate_poseidon_trace(&[perm_input]);

    let records = vec![
        evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap(),
        evaluate_chip("Poseidon", &PoseidonChip, &poseidon_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::PoseidonPermutation)
        .expect("C5 Execution Hash should balance with Poseidon");
}

// ── C4: MergeWriteSet bus (delete scenario) ──

#[test]
fn c4_merge_write_set_balanced_delete() {
    // SortedMem: init + write(null) → has_written=1, val_is_null=1.
    // Merge: delete_row receives write-set entry.
    // Write null = delete semantics: SortedMem send, Merge receive.
    let sm_rows = vec![
        init_row(0, 0, 100, [42, 0, 0], false),
        write_row(0, 0, 100, 1, [0, 0, 0], true), // null write = delete
    ];
    let sm_trace = generate_sorted_mem_trace::<3>(&sm_rows);

    // Merge delete_row receives (t=0, c=0, key=100, val=[0,0,0], is_null=1).
    let merge_rows = vec![MergeRow {
        table_id: 0,
        col_id: 0,
        key: 100,
        source: MergeSource::Delete,
        old_val: merge_val([42, 0, 0]),
        write_val: merge_zeros(),
        new_val: merge_zeros(),
        in_new: false,
        hash_acc: [BabyBear::ZERO; 8],
    }];
    let merge_trace = generate_merge_trace::<3>(&merge_rows);

    let records = vec![
        evaluate_chip("SortedMem", &GlobalSortedMemChip::<3>, &sm_trace).unwrap(),
        evaluate_chip("Merge", &GlobalMergeChip::<3>, &merge_trace).unwrap(),
    ];
    check_bus_balance(&records, InteractionKind::MergeWriteSet)
        .expect("C4 delete write-set should balance");
}
