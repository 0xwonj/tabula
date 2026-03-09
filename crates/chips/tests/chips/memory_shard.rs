//! Tests for the MemoryShard AIR chip.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;

use tabula_chips::shards::memory::air::MemoryShardChip;
use tabula_chips::shards::memory::columns::{
    MEMORY_SHARD_STANDARD_WIDTH, MemoryShardCols, memory_shard_width,
};
use tabula_chips::shards::memory::trace::{MemoryShardRow, generate_memory_shard_trace};
use tabula_stark::chips::ChipId;
use tabula_stark::air::borrow_cols_mut;
use tabula_stark::debug::debug_check;

use tabula_chips::test_utils::builders::{ms_init, ms_read, ms_read_write, ms_write};

fn chip() -> MemoryShardChip<3> {
    MemoryShardChip::new(ChipId(100), 0, 0)
}

fn trace(rows: &[MemoryShardRow]) -> RowMajorMatrix<BabyBear> {
    generate_memory_shard_trace::<3>(0, 0, rows)
}

// ── Column width ──

#[test]
fn standard_width_is_48() {
    assert_eq!(MEMORY_SHARD_STANDARD_WIDTH, 48);
}

#[test]
fn generic_width_matches() {
    assert_eq!(memory_shard_width::<3>(), 48);
}

// ── A. Valid single-key traces ──

#[test]
fn valid_init_only() {
    let rows = vec![ms_init(100, [50, 0, 0], false)];
    debug_check(&chip(), &trace(&rows)).expect("init only should pass");
}

#[test]
fn valid_init_then_read() {
    let rows = vec![
        ms_init(100, [50, 0, 0], false),
        ms_read(100, 0, [50, 0, 0], false),
    ];
    debug_check(&chip(), &trace(&rows)).expect("init+read should pass");
}

#[test]
fn valid_init_then_write() {
    let rows = vec![
        ms_init(100, [50, 0, 0], false),
        ms_write(100, 0, [50, 0, 0], false, [75, 0, 0], false),
    ];
    debug_check(&chip(), &trace(&rows)).expect("init+write should pass");
}

#[test]
fn valid_init_then_read_write() {
    let rows = vec![
        ms_init(100, [50, 0, 0], false),
        ms_read_write(100, 0, [50, 0, 0], false, [75, 0, 0], false),
    ];
    debug_check(&chip(), &trace(&rows)).expect("init+read_write should pass");
}

// ── B. Valid inter-tx chains ──

#[test]
fn valid_two_tx_write_then_read() {
    let rows = vec![
        ms_init(100, [50, 0, 0], false),
        ms_read_write(100, 0, [50, 0, 0], false, [75, 0, 0], false),
        ms_read(100, 1, [75, 0, 0], false),
    ];
    debug_check(&chip(), &trace(&rows)).expect("write→read chain should pass");
}

#[test]
fn valid_three_tx_chain() {
    let rows = vec![
        ms_init(100, [50, 0, 0], false),
        ms_read_write(100, 0, [50, 0, 0], false, [75, 0, 0], false),
        ms_read_write(100, 1, [75, 0, 0], false, [90, 0, 0], false),
        ms_read(100, 2, [90, 0, 0], false),
    ];
    debug_check(&chip(), &trace(&rows)).expect("3-tx chain should pass");
}

#[test]
fn valid_read_only_chain() {
    let rows = vec![
        ms_init(100, [50, 0, 0], false),
        ms_read(100, 0, [50, 0, 0], false),
        ms_read(100, 1, [50, 0, 0], false),
        ms_read(100, 2, [50, 0, 0], false),
    ];
    debug_check(&chip(), &trace(&rows)).expect("read-only chain should pass");
}

#[test]
fn valid_write_only_then_read() {
    let rows = vec![
        ms_init(100, [50, 0, 0], false),
        ms_write(100, 0, [50, 0, 0], false, [75, 0, 0], false),
        ms_read(100, 1, [75, 0, 0], false),
    ];
    debug_check(&chip(), &trace(&rows)).expect("write-only→read should pass");
}

// ── C. Valid multi-key traces ──

#[test]
fn valid_two_keys() {
    let rows = vec![
        ms_init(100, [50, 0, 0], false),
        ms_read(100, 0, [50, 0, 0], false),
        ms_init(200, [30, 0, 0], false),
        ms_read(200, 0, [30, 0, 0], false),
    ];
    debug_check(&chip(), &trace(&rows)).expect("two keys should pass");
}

#[test]
fn valid_many_keys() {
    let rows = vec![
        ms_init(10, [1, 0, 0], false),
        ms_init(20, [2, 0, 0], false),
        ms_init(30, [3, 0, 0], false),
    ];
    debug_check(&chip(), &trace(&rows)).expect("many init-only keys should pass");
}

#[test]
fn valid_null_values() {
    let rows = vec![
        ms_init(100, [0, 0, 0], true),
        ms_read(100, 0, [0, 0, 0], true),
    ];
    debug_check(&chip(), &trace(&rows)).expect("null values should pass");
}

#[test]
fn valid_write_null_delete() {
    let rows = vec![
        ms_init(100, [50, 0, 0], false),
        ms_read_write(100, 0, [50, 0, 0], false, [0, 0, 0], true),
    ];
    debug_check(&chip(), &trace(&rows)).expect("write null (delete) should pass");
}

#[test]
fn valid_non_sequential_tx_indices() {
    let rows = vec![
        ms_init(100, [50, 0, 0], false),
        ms_read(100, 0, [50, 0, 0], false),
        ms_read(100, 5, [50, 0, 0], false),
    ];
    debug_check(&chip(), &trace(&rows)).expect("non-sequential tx indices should pass");
}

#[test]
fn valid_large_keys() {
    let rows = vec![
        ms_init(1 << 40, [1, 0, 0], false),
        ms_read(1 << 40, 0, [1, 0, 0], false),
        ms_init((1 << 40) + 1000, [2, 0, 0], false),
    ];
    debug_check(&chip(), &trace(&rows)).expect("large keys should pass");
}

#[test]
fn valid_has_ever_written_propagation() {
    let rows = vec![
        ms_init(100, [50, 0, 0], false),
        ms_write(100, 0, [50, 0, 0], false, [75, 0, 0], false),
        ms_read(100, 1, [75, 0, 0], false),
    ];
    debug_check(&chip(), &trace(&rows)).expect("has_ever_written propagation should pass");
}

// ── D. Invalid traces ──

#[test]
fn invalid_missing_init() {
    let rows = vec![ms_read(100, 0, [50, 0, 0], false)];
    debug_check(&chip(), &trace(&rows)).expect_err("missing init should fail");
}

#[test]
fn invalid_read_inconsistency() {
    let rows = vec![
        ms_init(100, [50, 0, 0], false),
        ms_read_write(100, 0, [50, 0, 0], false, [75, 0, 0], false),
        ms_read(100, 1, [99, 0, 0], false), // WRONG: should be 75
    ];
    debug_check(&chip(), &trace(&rows)).expect_err("read inconsistency should fail");
}

#[test]
fn invalid_output_derivation() {
    let mut rows = vec![
        ms_init(100, [50, 0, 0], false),
        ms_read(100, 0, [50, 0, 0], false),
    ];
    rows[1].output_val = vec![BabyBear::new(99), BabyBear::ZERO, BabyBear::ZERO];
    debug_check(&chip(), &trace(&rows)).expect_err("output derivation error should fail");
}

#[test]
fn invalid_init_with_has_read() {
    let mut rows = vec![ms_init(100, [50, 0, 0], false)];
    rows[0].has_read = true;
    debug_check(&chip(), &trace(&rows)).expect_err("init with has_read should fail");
}

#[test]
fn invalid_init_with_has_write() {
    let mut rows = vec![ms_init(100, [50, 0, 0], false)];
    rows[0].has_write = true;
    debug_check(&chip(), &trace(&rows)).expect_err("init with has_write should fail");
}

#[test]
fn invalid_no_read_no_write() {
    let rows = vec![
        ms_init(100, [50, 0, 0], false),
        MemoryShardRow {
            key: 100,
            tx_index: 0,
            is_init: false,
            has_read: false,
            has_write: false,
            input_val: vec![BabyBear::new(50), BabyBear::ZERO, BabyBear::ZERO],
            input_is_null: false,
            output_val: vec![BabyBear::new(50), BabyBear::ZERO, BabyBear::ZERO],
            output_is_null: false,
        },
    ];
    debug_check(&chip(), &trace(&rows)).expect_err("no read/write should fail");
}

#[test]
fn invalid_key_ordering_violation() {
    let t = build_manual_key_ordering_violation();
    debug_check(&chip(), &t).expect_err("key ordering violation should fail");
}

#[test]
fn invalid_init_output_differs_from_input() {
    let mut rows = vec![ms_init(100, [50, 0, 0], false)];
    rows[0].output_val = vec![BabyBear::new(99), BabyBear::ZERO, BabyBear::ZERO];
    debug_check(&chip(), &trace(&rows)).expect_err("init output≠input should fail");
}

#[test]
fn invalid_null_inconsistency_in_read() {
    let rows = vec![
        ms_init(100, [50, 0, 0], false),
        ms_read(100, 0, [50, 0, 0], true), // null mismatch
    ];
    debug_check(&chip(), &trace(&rows)).expect_err("null inconsistency should fail");
}

#[test]
fn invalid_forged_has_ever_written() {
    let rows = vec![
        ms_init(100, [50, 0, 0], false),
        ms_read(100, 0, [50, 0, 0], false),
    ];
    let mut t = trace(&rows);

    // Corrupt: set has_ever_written=1 on the read row (row 1)
    let width = memory_shard_width::<3>();
    let cols: &mut MemoryShardCols<BabyBear, 3> =
        borrow_cols_mut(&mut t.values[width..2 * width]);
    cols.has_ever_written = BabyBear::ONE;

    debug_check(&chip(), &t).expect_err("forged has_ever_written should fail");
}

// ── E. Constant identity constraint ──

#[test]
fn invalid_table_id_change() {
    // Build a trace where table_id changes mid-trace.
    let width = memory_shard_width::<3>();
    let num_rows = 4;
    let mut values = vec![BabyBear::ZERO; num_rows * width];

    // Row 0: init, table_id=0
    {
        let cols: &mut MemoryShardCols<BabyBear, 3> = borrow_cols_mut(&mut values[0..width]);
        cols.is_real = BabyBear::ONE;
        cols.table_id = BabyBear::ZERO;
        cols.key.populate(100);
        cols.is_init = BabyBear::ONE;
        cols.input_val = [BabyBear::new(50), BabyBear::ZERO, BabyBear::ZERO];
        cols.output_val = [BabyBear::new(50), BabyBear::ZERO, BabyBear::ZERO];
        cols.is_last_for_key = BabyBear::ONE;
    }

    // Row 1: init, table_id=1 (VIOLATION)
    {
        let cols: &mut MemoryShardCols<BabyBear, 3> =
            borrow_cols_mut(&mut values[width..2 * width]);
        cols.is_real = BabyBear::ONE;
        cols.table_id = BabyBear::ONE; // Different table!
        cols.key.populate(100);
        cols.is_init = BabyBear::ONE;
        cols.input_val = [BabyBear::new(50), BabyBear::ZERO, BabyBear::ZERO];
        cols.output_val = [BabyBear::new(50), BabyBear::ZERO, BabyBear::ZERO];
        cols.is_last_for_key = BabyBear::ONE;
    }

    // Populate IsZero witnesses for all rows (including padding)
    for i in 0..num_rows {
        let next_idx = (i + 1) % num_rows;
        let cur_key = if i < 2 { 100u64 } else { 0 };
        let next_key = if next_idx < 2 { 100u64 } else { 0 };

        let offset = i * width;
        let cols: &mut MemoryShardCols<BabyBear, 3> =
            borrow_cols_mut(&mut values[offset..offset + width]);

        let diff0 = BabyBear::new((next_key & 0x3FFF_FFFF) as u32)
            - BabyBear::new((cur_key & 0x3FFF_FFFF) as u32);
        let diff1 = BabyBear::new(((next_key >> 30) & 0x3FFF_FFFF) as u32)
            - BabyBear::new(((cur_key >> 30) & 0x3FFF_FFFF) as u32);
        let diff2 =
            BabyBear::new((next_key >> 60) as u32) - BabyBear::new((cur_key >> 60) as u32);
        cols.r_limb0_iz.populate(diff0);
        cols.r_limb1_iz.populate(diff1);
        cols.r_limb2_iz.populate(diff2);
    }

    let t = RowMajorMatrix::new(values, width);
    debug_check(&chip(), &t).expect_err("table_id change should fail");
}

// ── F. Different (t,c) chip instances ──

#[test]
fn valid_different_column_chip() {
    // MemoryShard for table=5, col=3
    let c = MemoryShardChip::<3>::new(ChipId(101), 5, 3);
    let rows = vec![
        ms_init(100, [50, 0, 0], false),
        ms_read(100, 0, [50, 0, 0], false),
    ];
    let t = generate_memory_shard_trace::<3>(5, 3, &rows);
    debug_check(&c, &t).expect("different column chip should pass");
}

// ── Helper functions ──

fn build_manual_key_ordering_violation() -> RowMajorMatrix<BabyBear> {
    let width = memory_shard_width::<3>();
    let num_rows = 4;
    let mut values = vec![BabyBear::ZERO; num_rows * width];

    // Row 0: init, key=200
    {
        let cols: &mut MemoryShardCols<BabyBear, 3> = borrow_cols_mut(&mut values[0..width]);
        cols.is_real = BabyBear::ONE;
        cols.key.populate(200);
        cols.is_init = BabyBear::ONE;
        cols.input_val = [BabyBear::new(1), BabyBear::ZERO, BabyBear::ZERO];
        cols.output_val = [BabyBear::new(1), BabyBear::ZERO, BabyBear::ZERO];
        cols.is_last_for_key = BabyBear::ONE;
    }

    // Row 1: init, key=100 (violation: 200 > 100)
    {
        let cols: &mut MemoryShardCols<BabyBear, 3> =
            borrow_cols_mut(&mut values[width..2 * width]);
        cols.is_real = BabyBear::ONE;
        cols.key.populate(100);
        cols.is_init = BabyBear::ONE;
        cols.input_val = [BabyBear::new(2), BabyBear::ZERO, BabyBear::ZERO];
        cols.output_val = [BabyBear::new(2), BabyBear::ZERO, BabyBear::ZERO];
        cols.is_last_for_key = BabyBear::ONE;
    }

    // Populate IsZero witnesses
    for i in 0..num_rows {
        let next_idx = (i + 1) % num_rows;
        let cur_key: u64 = match i {
            0 => 200,
            1 => 100,
            _ => 0,
        };
        let next_key: u64 = match next_idx {
            0 => 200,
            1 => 100,
            _ => 0,
        };

        let offset = i * width;
        let cols: &mut MemoryShardCols<BabyBear, 3> =
            borrow_cols_mut(&mut values[offset..offset + width]);

        let diff0 = BabyBear::new((next_key & 0x3FFF_FFFF) as u32)
            - BabyBear::new((cur_key & 0x3FFF_FFFF) as u32);
        let diff1 = BabyBear::new(((next_key >> 30) & 0x3FFF_FFFF) as u32)
            - BabyBear::new(((cur_key >> 30) & 0x3FFF_FFFF) as u32);
        let diff2 =
            BabyBear::new((next_key >> 60) as u32) - BabyBear::new((cur_key >> 60) as u32);
        cols.r_limb0_iz.populate(diff0);
        cols.r_limb1_iz.populate(diff1);
        cols.r_limb2_iz.populate(diff2);
    }

    RowMajorMatrix::new(values, width)
}
