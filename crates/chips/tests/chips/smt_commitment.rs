//! Tests for the SmtCommitment bundled ColumnCommitment impl.

use p3_matrix::Matrix;
use tabula_core::{ColId, TableId};

use tabula_chips::shards::ChipIdAllocator;
use tabula_chips::shards::smt::{SMT_WITNESS_LABEL, SmtColumnWitness, SmtCommitment, SmtWitness};
use tabula_chips::test_utils::builders::{ms_init, ms_touched};
use tabula_chips::test_utils::values::distinct_digest;

use tabula_stark::trace::column_commitment::{ColumnCommitment, ColumnPlan, EncodingWidth};
use tabula_stark::trace::contributor::WitnessStore;

fn plan(table: u32, col: u16) -> ColumnPlan {
    ColumnPlan {
        table: TableId(table),
        col: ColId(col),
        encoding_width: EncodingWidth::STANDARD,
        scheme_name: "smt".to_string(),
    }
}

// ── Chip ID allocation ──

#[test]
fn chip_ids_returns_two_per_column() {
    let plans = vec![plan(0, 0)];
    let mut alloc = ChipIdAllocator::for_shards();
    let smt = SmtCommitment::<3>::new(&plans, &mut alloc);

    let ids = smt.chip_ids();
    assert_eq!(ids.len(), 2); // 1 column × 2 chips each
    assert_eq!(ids[0].tag(), 100); // memory
    assert_eq!(ids[1].tag(), 101); // meta
}

#[test]
fn chip_ids_scales_with_columns() {
    let plans = vec![plan(0, 0), plan(0, 1), plan(1, 0)];
    let mut alloc = ChipIdAllocator::for_shards();
    let smt = SmtCommitment::<3>::new(&plans, &mut alloc);

    let ids = smt.chip_ids();
    // 3 SMT columns × 2 chips each = 6 total
    assert_eq!(ids.len(), 6);
}

#[test]
fn chip_ids_skips_non_smt_during_construction() {
    let plans = vec![
        plan(0, 0),
        ColumnPlan {
            table: TableId(0),
            col: ColId(1),
            encoding_width: EncodingWidth::STANDARD,
            scheme_name: "ssmc".to_string(),
        },
        plan(1, 0),
    ];
    let mut alloc = ChipIdAllocator::for_shards();
    let smt = SmtCommitment::<3>::new(&plans, &mut alloc);

    // Only 2 SMT columns registered → 4 IDs total
    let ids = smt.chip_ids();
    assert_eq!(ids.len(), 4);
}

// ── Name ──

#[test]
fn name_is_smt() {
    let mut alloc = ChipIdAllocator::for_shards();
    let smt = SmtCommitment::<3>::new(&[], &mut alloc);
    assert_eq!(smt.name(), "smt");
}

// ── Output buses ──

#[test]
fn output_buses_nonempty() {
    let mut alloc = ChipIdAllocator::for_shards();
    let smt = SmtCommitment::<3>::new(&[], &mut alloc);
    assert!(!smt.output_buses().is_empty());
}

// ── Build traces ──

#[test]
fn build_traces_produces_two_entries() {
    let plans = vec![plan(0, 0)];
    let mut alloc = ChipIdAllocator::for_shards();
    let smt = SmtCommitment::<3>::new(&plans, &mut alloc);

    let d1 = distinct_digest(1);
    let d2 = distinct_digest(2);

    let mut witness = SmtWitness::default();
    witness.insert(
        TableId(0),
        ColId(0),
        SmtColumnWitness {
            memory_rows: vec![ms_init(100, [50, 0, 0], false)],
            meta_row: Some(ms_touched(d1, d2)),
        },
    );

    let mut store = WitnessStore::new();
    store.put(SMT_WITNESS_LABEL, witness);

    let entries = smt.build_traces(&plans[0..1], &store).unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries[0].1.main.height() >= 2);
    assert!(entries[1].1.main.height() >= 2);
}

#[test]
fn build_traces_missing_witness_errors() {
    let plans = vec![plan(0, 0)];
    let mut alloc = ChipIdAllocator::for_shards();
    let smt = SmtCommitment::<3>::new(&plans, &mut alloc);
    let store = WitnessStore::new();
    assert!(smt.build_traces(&plans[0..1], &store).is_err());
}

#[test]
fn build_traces_unknown_column_errors() {
    let plans = vec![plan(0, 0)];
    let mut alloc = ChipIdAllocator::for_shards();
    let smt = SmtCommitment::<3>::new(&plans, &mut alloc);

    let mut witness = SmtWitness::default();
    witness.insert(
        TableId(0),
        ColId(0),
        SmtColumnWitness {
            memory_rows: vec![],
            meta_row: None,
        },
    );

    let mut store = WitnessStore::new();
    store.put(SMT_WITNESS_LABEL, witness);

    let unknown = plan(99, 99);
    // Passing an unknown column to build_traces should error (no column index).
    assert!(smt.build_traces(&[unknown], &store).is_err());
}

#[test]
fn build_traces_empty_column() {
    let plans = vec![plan(0, 0)];
    let mut alloc = ChipIdAllocator::for_shards();
    let smt = SmtCommitment::<3>::new(&plans, &mut alloc);

    let mut witness = SmtWitness::default();
    witness.insert(
        TableId(0),
        ColId(0),
        SmtColumnWitness {
            memory_rows: vec![],
            meta_row: None,
        },
    );

    let mut store = WitnessStore::new();
    store.put(SMT_WITNESS_LABEL, witness);

    let entries = smt.build_traces(&plans[0..1], &store).unwrap();
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].1.main.height(), 2);
    assert_eq!(entries[1].1.main.height(), 2);
}
