//! Tests for the SsmcCommitment bundled ColumnCommitment impl.

use p3_matrix::Matrix;
use tabula_core::{ColId, TableId};

use tabula_chips::shards::ssmc::{
    SSMC_WITNESS_LABEL, SsmcColumnWitness, SsmcCommitment, SsmcWitness,
};
use tabula_chips::shards::ChipIdAllocator;
use tabula_chips::test_utils::builders::{ms_init, ms_touched, ss_old_only};
use tabula_chips::test_utils::values::distinct_digest;

use tabula_stark::trace::column_commitment::{ColumnCommitment, ColumnPlan, EncodingWidth};
use tabula_stark::trace::contributor::WitnessStore;

fn plan(table: u32, col: u16) -> ColumnPlan {
    ColumnPlan {
        table: TableId(table),
        col: ColId(col),
        encoding_width: EncodingWidth::STANDARD,
        scheme_name: "ssmc".to_string(),
    }
}

// ── Chip ID allocation ──

#[test]
fn chip_ids_returns_three_per_column() {
    let plans = vec![plan(0, 0), plan(0, 1), plan(1, 0)];
    let mut alloc = ChipIdAllocator::for_shards();
    let ssmc = SsmcCommitment::<3>::new(&plans, &mut alloc);

    let ids = ssmc.chip_ids();
    // 3 SSMC columns × 3 chips each (memory, state, meta) = 9
    assert_eq!(ids.len(), 9);
}

#[test]
fn chip_ids_single_column() {
    let plans = vec![plan(0, 0)];
    let mut alloc = ChipIdAllocator::for_shards();
    let ssmc = SsmcCommitment::<3>::new(&plans, &mut alloc);

    let ids = ssmc.chip_ids();
    assert_eq!(ids.len(), 3);
    // First column gets IDs 100, 101, 102
    assert_eq!(ids[0].tag(), 100); // memory
    assert_eq!(ids[1].tag(), 101); // state
    assert_eq!(ids[2].tag(), 102); // meta
}

#[test]
fn chip_ids_two_columns_returns_six() {
    let plans = vec![plan(0, 0), plan(0, 1)];
    let mut alloc = ChipIdAllocator::for_shards();
    let ssmc = SsmcCommitment::<3>::new(&plans, &mut alloc);

    let ids = ssmc.chip_ids();
    // 2 columns × 3 chips each = 6
    assert_eq!(ids.len(), 6);
    // First column: 100, 101, 102
    assert_eq!(ids[0].tag(), 100);
    assert_eq!(ids[1].tag(), 101);
    assert_eq!(ids[2].tag(), 102);
    // Second column: 103, 104, 105
    assert_eq!(ids[3].tag(), 103);
    assert_eq!(ids[4].tag(), 104);
    assert_eq!(ids[5].tag(), 105);
}

#[test]
fn chip_ids_skips_non_ssmc() {
    let plans = vec![
        plan(0, 0),
        ColumnPlan {
            table: TableId(0),
            col: ColId(1),
            encoding_width: EncodingWidth::STANDARD,
            scheme_name: "smt".to_string(), // not SSMC
        },
        plan(1, 0),
    ];
    let mut alloc = ChipIdAllocator::for_shards();
    let ssmc = SsmcCommitment::<3>::new(&plans, &mut alloc);

    // (0,0) → 3 IDs, (1,0) → 3 IDs (SMT column skipped) = 6 total
    let ids = ssmc.chip_ids();
    assert_eq!(ids.len(), 6);
}

// ── Name ──

#[test]
fn name_is_ssmc() {
    let mut alloc = ChipIdAllocator::for_shards();
    let ssmc = SsmcCommitment::<3>::new(&[], &mut alloc);
    assert_eq!(ssmc.name(), "ssmc");
}

// ── Output buses ──

#[test]
fn output_buses_nonempty() {
    let mut alloc = ChipIdAllocator::for_shards();
    let ssmc = SsmcCommitment::<3>::new(&[], &mut alloc);
    assert!(!ssmc.output_buses().is_empty());
}

// ── Build traces ──

#[test]
fn build_traces_produces_three_entries() {
    let plans = vec![plan(0, 0)];
    let mut alloc = ChipIdAllocator::for_shards();
    let ssmc = SsmcCommitment::<3>::new(&plans, &mut alloc);

    let d1 = distinct_digest(1);
    let d2 = distinct_digest(2);

    let mut witness = SsmcWitness::default();
    witness.insert(
        TableId(0),
        ColId(0),
        SsmcColumnWitness {
            memory_rows: vec![ms_init(100, [50, 0, 0], false)],
            state_rows: vec![ss_old_only(100, [50, 0, 0])],
            meta_row: Some(ms_touched(d1, d2)),
        },
    );

    let mut store = WitnessStore::new();
    store.put(SSMC_WITNESS_LABEL, witness);

    let entries = ssmc.build_traces(&plans[0..1], &store).unwrap();
    assert_eq!(entries.len(), 3);

    // Verify traces are non-empty
    assert!(entries[0].1.main.height() >= 2);
    assert!(entries[1].1.main.height() >= 2);
    assert!(entries[2].1.main.height() >= 2);
}

#[test]
fn build_traces_missing_witness_errors() {
    let plans = vec![plan(0, 0)];
    let mut alloc = ChipIdAllocator::for_shards();
    let ssmc = SsmcCommitment::<3>::new(&plans, &mut alloc);

    let store = WitnessStore::new(); // empty store
    let result = ssmc.build_traces(&plans[0..1], &store);
    assert!(result.is_err());
}

#[test]
fn build_traces_missing_column_data_errors() {
    let plans = vec![plan(0, 0)];
    let mut alloc = ChipIdAllocator::for_shards();
    let ssmc = SsmcCommitment::<3>::new(&plans, &mut alloc);

    let witness = SsmcWitness::default(); // no column data
    let mut store = WitnessStore::new();
    store.put(SSMC_WITNESS_LABEL, witness);

    let result = ssmc.build_traces(&plans[0..1], &store);
    assert!(result.is_err());
}

#[test]
fn build_traces_empty_column() {
    let plans = vec![plan(0, 0)];
    let mut alloc = ChipIdAllocator::for_shards();
    let ssmc = SsmcCommitment::<3>::new(&plans, &mut alloc);

    let mut witness = SsmcWitness::default();
    witness.insert(
        TableId(0),
        ColId(0),
        SsmcColumnWitness {
            memory_rows: vec![],
            state_rows: vec![],
            meta_row: None,
        },
    );

    let mut store = WitnessStore::new();
    store.put(SSMC_WITNESS_LABEL, witness);

    let entries = ssmc.build_traces(&plans[0..1], &store).unwrap();
    assert_eq!(entries.len(), 3);
    // All traces should be padding-only (height=2 minimum)
    assert_eq!(entries[0].1.main.height(), 2);
    assert_eq!(entries[1].1.main.height(), 2);
    assert_eq!(entries[2].1.main.height(), 2);
}
