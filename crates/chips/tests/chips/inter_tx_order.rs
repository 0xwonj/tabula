//! Tests for the InterTxOrder AIR chip.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrix;

use tabula_chips::inter_tx_order::air::InterTxOrderChip;
use tabula_chips::inter_tx_order::columns::{
    INTER_TX_ORDER_STANDARD_WIDTH, InterTxOrderCols, inter_tx_order_width,
};
use tabula_chips::inter_tx_order::trace::{InterTxOrderRow, generate_inter_tx_order_trace};
use tabula_stark::air::borrow_cols_mut;
use tabula_stark::debug::debug_check;

use tabula_chips::test_utils::builders::{ito_init, ito_read, ito_read_write, ito_write};

// ── Column width ──

#[test]
fn standard_width_is_56() {
    assert_eq!(INTER_TX_ORDER_STANDARD_WIDTH, 56);
}

#[test]
fn generic_width_matches() {
    assert_eq!(inter_tx_order_width::<3>(), 56);
}

// ── A. Valid single-key traces ──

#[test]
fn valid_init_only() {
    let rows = vec![ito_init(0, 0, 100, [50, 0, 0], false)];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect("init only should pass");
}

#[test]
fn valid_init_then_read() {
    let rows = vec![
        ito_init(0, 0, 100, [50, 0, 0], false),
        ito_read(0, 0, 100, 0, [50, 0, 0], false),
    ];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect("init+read should pass");
}

#[test]
fn valid_init_then_write() {
    let rows = vec![
        ito_init(0, 0, 100, [50, 0, 0], false),
        ito_write(0, 0, 100, 0, [50, 0, 0], false, [75, 0, 0], false),
    ];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect("init+write should pass");
}

#[test]
fn valid_init_then_read_write() {
    let rows = vec![
        ito_init(0, 0, 100, [50, 0, 0], false),
        ito_read_write(0, 0, 100, 0, [50, 0, 0], false, [75, 0, 0], false),
    ];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect("init+read_write should pass");
}

// ── B. Valid inter-tx chains ──

#[test]
fn valid_two_tx_write_then_read() {
    // tx_0 writes 75, tx_1 reads 75
    let rows = vec![
        ito_init(0, 0, 100, [50, 0, 0], false),
        ito_read_write(0, 0, 100, 0, [50, 0, 0], false, [75, 0, 0], false),
        ito_read(0, 0, 100, 1, [75, 0, 0], false),
    ];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect("write→read chain should pass");
}

#[test]
fn valid_three_tx_chain() {
    // init(50) → tx0 writes 75 → tx1 writes 90 → tx2 reads 90
    let rows = vec![
        ito_init(0, 0, 100, [50, 0, 0], false),
        ito_read_write(0, 0, 100, 0, [50, 0, 0], false, [75, 0, 0], false),
        ito_read_write(0, 0, 100, 1, [75, 0, 0], false, [90, 0, 0], false),
        ito_read(0, 0, 100, 2, [90, 0, 0], false),
    ];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect("3-tx chain should pass");
}

#[test]
fn valid_read_only_chain() {
    // Multiple txs all just read the same key
    let rows = vec![
        ito_init(0, 0, 100, [50, 0, 0], false),
        ito_read(0, 0, 100, 0, [50, 0, 0], false),
        ito_read(0, 0, 100, 1, [50, 0, 0], false),
        ito_read(0, 0, 100, 2, [50, 0, 0], false),
    ];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect("read-only chain should pass");
}

#[test]
fn valid_write_only_then_read() {
    // tx_0 writes without reading, tx_1 reads
    let rows = vec![
        ito_init(0, 0, 100, [50, 0, 0], false),
        ito_write(0, 0, 100, 0, [50, 0, 0], false, [75, 0, 0], false),
        ito_read(0, 0, 100, 1, [75, 0, 0], false),
    ];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect("write-only→read should pass");
}

// ── C. Valid multi-key traces ──

#[test]
fn valid_two_keys_same_segment() {
    let rows = vec![
        ito_init(0, 0, 100, [50, 0, 0], false),
        ito_read(0, 0, 100, 0, [50, 0, 0], false),
        ito_init(0, 0, 200, [30, 0, 0], false),
        ito_read(0, 0, 200, 0, [30, 0, 0], false),
    ];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect("two keys same segment should pass");
}

#[test]
fn valid_multi_segment() {
    let rows = vec![
        // Segment (0,0)
        ito_init(0, 0, 100, [50, 0, 0], false),
        ito_read(0, 0, 100, 0, [50, 0, 0], false),
        // Segment (0,1)
        ito_init(0, 1, 100, [10, 0, 0], false),
        ito_read_write(0, 1, 100, 0, [10, 0, 0], false, [20, 0, 0], false),
        // Segment (1,0)
        ito_init(1, 0, 50, [5, 0, 0], false),
    ];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect("multi segment should pass");
}

#[test]
fn valid_null_values() {
    // Init with null, tx reads null
    let rows = vec![
        ito_init(0, 0, 100, [0, 0, 0], true),
        ito_read(0, 0, 100, 0, [0, 0, 0], true),
    ];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect("null values should pass");
}

#[test]
fn valid_write_null_delete() {
    // Init with value, tx writes null (delete)
    let rows = vec![
        ito_init(0, 0, 100, [50, 0, 0], false),
        ito_read_write(0, 0, 100, 0, [50, 0, 0], false, [0, 0, 0], true),
    ];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect("write null (delete) should pass");
}

#[test]
fn valid_non_sequential_tx_indices() {
    // tx_index 0 and tx_index 5 (gap is ok — tx_diff just needs to be in range)
    let rows = vec![
        ito_init(0, 0, 100, [50, 0, 0], false),
        ito_read(0, 0, 100, 0, [50, 0, 0], false),
        ito_read(0, 0, 100, 5, [50, 0, 0], false),
    ];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect("non-sequential tx indices should pass");
}

#[test]
fn valid_large_keys() {
    let rows = vec![
        ito_init(0, 0, 1 << 40, [1, 0, 0], false),
        ito_read(0, 0, 1 << 40, 0, [1, 0, 0], false),
        ito_init(0, 0, (1 << 40) + 1000, [2, 0, 0], false),
    ];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect("large keys should pass");
}

// ── D. Invalid traces ──

#[test]
fn invalid_missing_init() {
    // Access row without init
    let rows = vec![ito_read(0, 0, 100, 0, [50, 0, 0], false)];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace)
        .expect_err("missing init should fail (first row must be init)");
}

#[test]
fn invalid_read_inconsistency() {
    // tx_1 reads 99 but prev output was 75
    let rows = vec![
        ito_init(0, 0, 100, [50, 0, 0], false),
        ito_read_write(0, 0, 100, 0, [50, 0, 0], false, [75, 0, 0], false),
        ito_read(0, 0, 100, 1, [99, 0, 0], false), // WRONG: should be 75
    ];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect_err("read inconsistency should fail");
}

#[test]
fn invalid_output_derivation() {
    // Read-only tx but output differs from input
    let mut rows = vec![
        ito_init(0, 0, 100, [50, 0, 0], false),
        ito_read(0, 0, 100, 0, [50, 0, 0], false),
    ];
    // Manually corrupt: make output differ from input for read-only
    rows[1].output_val = vec![BabyBear::new(99), BabyBear::ZERO, BabyBear::ZERO];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect_err("output derivation error should fail");
}

#[test]
fn invalid_init_with_has_read() {
    // Init row claims has_read=true
    let mut rows = vec![ito_init(0, 0, 100, [50, 0, 0], false)];
    rows[0].has_read = true;
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect_err("init with has_read should fail");
}

#[test]
fn invalid_init_with_has_write() {
    // Init row claims has_write=true
    let mut rows = vec![ito_init(0, 0, 100, [50, 0, 0], false)];
    rows[0].has_write = true;
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect_err("init with has_write should fail");
}

#[test]
fn invalid_no_read_no_write() {
    // Non-init row with neither read nor write
    let rows = vec![
        ito_init(0, 0, 100, [50, 0, 0], false),
        InterTxOrderRow {
            table_id: 0,
            col_id: 0,
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
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect_err("no read/write should fail");
}

#[test]
fn invalid_key_ordering_violation() {
    // key 200 before key 100 in same segment.
    // Can't use generate_trace (StrictIneq::populate panics on 200 > 100),
    // so we build the trace manually with wrong ordering witnesses.
    let trace = build_manual_key_ordering_violation();
    debug_check(&InterTxOrderChip::<3>, &trace).expect_err("key ordering violation should fail");
}

#[test]
fn invalid_init_output_differs_from_input() {
    // Init row where output != input
    let mut rows = vec![ito_init(0, 0, 100, [50, 0, 0], false)];
    rows[0].output_val = vec![BabyBear::new(99), BabyBear::ZERO, BabyBear::ZERO];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect_err("init output≠input should fail");
}

#[test]
fn invalid_null_inconsistency_in_read() {
    // tx reads with is_null=true but prev output was is_null=false
    let rows = vec![
        ito_init(0, 0, 100, [50, 0, 0], false),
        ito_read(0, 0, 100, 0, [50, 0, 0], true), // null mismatch
    ];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect_err("null inconsistency should fail");
}

// ── E. Multiple inits for different keys ──

#[test]
fn valid_many_keys_many_segments() {
    let rows = vec![
        ito_init(0, 0, 10, [1, 0, 0], false),
        ito_init(0, 0, 20, [2, 0, 0], false),
        ito_init(0, 0, 30, [3, 0, 0], false),
        ito_init(0, 1, 10, [4, 0, 0], false),
        ito_init(1, 0, 10, [5, 0, 0], false),
    ];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect("many init-only keys should pass");
}

#[test]
fn valid_has_ever_written_propagation() {
    // tx_0 writes, tx_1 reads (has_ever_written stays true)
    let rows = vec![
        ito_init(0, 0, 100, [50, 0, 0], false),
        ito_write(0, 0, 100, 0, [50, 0, 0], false, [75, 0, 0], false),
        ito_read(0, 0, 100, 1, [75, 0, 0], false),
    ];
    let trace = generate_inter_tx_order_trace::<3>(&rows);
    debug_check(&InterTxOrderChip::<3>, &trace).expect("has_ever_written propagation should pass");
}

// ── F. has_ever_written forgery rejected ──

#[test]
fn invalid_forged_has_ever_written() {
    // A read-only chain where has_ever_written is falsely set to 1.
    // Before the O1 fix, this would pass (under-constrained).
    // After O1, the OR-propagation constraint rejects it:
    //   next.hew = local.hew(0) OR next.has_write(0) = 0, but witness says 1.
    let rows = vec![
        ito_init(0, 0, 100, [50, 0, 0], false),
        ito_read(0, 0, 100, 0, [50, 0, 0], false),
    ];
    let mut trace = generate_inter_tx_order_trace::<3>(&rows);

    // Corrupt: set has_ever_written=1 on the read row (row 1)
    let width = inter_tx_order_width::<3>();
    let cols: &mut InterTxOrderCols<BabyBear, 3> =
        borrow_cols_mut(&mut trace.values[width..2 * width]);
    cols.has_ever_written = BabyBear::ONE;

    debug_check(&InterTxOrderChip::<3>, &trace).expect_err("forged has_ever_written should fail");
}

// ── Helper functions ──

/// Build a trace with manually corrupted key ordering (key 200 before key 100).
fn build_manual_key_ordering_violation() -> RowMajorMatrix<BabyBear> {
    // We need to build a trace where key ordering is wrong but the trace
    // is otherwise structurally valid. The ordering columns will have wrong
    // witnesses, causing StrictIneq to fail.
    let width = inter_tx_order_width::<3>();
    let num_rows = 4; // power of 2
    let mut values = vec![BabyBear::ZERO; num_rows * width];

    // Row 0: init, key=200
    {
        let cols: &mut InterTxOrderCols<BabyBear, 3> = borrow_cols_mut(&mut values[0..width]);
        cols.is_real = BabyBear::ONE;
        cols.table_id = BabyBear::ZERO;
        cols.col_id = BabyBear::ZERO;
        cols.key.populate(200);
        cols.is_init = BabyBear::ONE;
        cols.input_val = [BabyBear::new(1), BabyBear::ZERO, BabyBear::ZERO];
        cols.output_val = [BabyBear::new(1), BabyBear::ZERO, BabyBear::ZERO];
        cols.is_last_for_key = BabyBear::ONE;
    }

    // Row 1: init, key=100 (violation: 200 > 100)
    {
        let cols: &mut InterTxOrderCols<BabyBear, 3> =
            borrow_cols_mut(&mut values[width..2 * width]);
        cols.is_real = BabyBear::ONE;
        cols.table_id = BabyBear::ZERO;
        cols.col_id = BabyBear::ZERO;
        cols.key.populate(100);
        cols.is_init = BabyBear::ONE;
        cols.input_val = [BabyBear::new(2), BabyBear::ZERO, BabyBear::ZERO];
        cols.output_val = [BabyBear::new(2), BabyBear::ZERO, BabyBear::ZERO];
        cols.is_last_for_key = BabyBear::ONE;
    }

    // Populate same_tc, key limb IsZero, and ordering witnesses for row 0
    // same_tc: table and col are same → tc_changed=0
    {
        let cols: &mut InterTxOrderCols<BabyBear, 3> = borrow_cols_mut(&mut values[0..width]);
        cols.same_tc.populate(BabyBear::ZERO, BabyBear::ZERO);

        // Key limb diffs: next_key=100, cur_key=200
        let limb0_diff = BabyBear::new((100u64 & 0x3FFFFFFF) as u32)
            - BabyBear::new((200u64 & 0x3FFFFFFF) as u32);
        cols.r_limb0_iz.populate(limb0_diff);
        cols.r_limb1_iz.populate(BabyBear::ZERO); // same upper limbs
        cols.r_limb2_iz.populate(BabyBear::ZERO);

        // key_ordering: can't populate because 200 < 100 is false
        // Leave as zeros — this will cause StrictIneq constraint to fail
    }

    // Row 1 segment detection (row 1 → row 2 which is padding)
    {
        let cols: &mut InterTxOrderCols<BabyBear, 3> =
            borrow_cols_mut(&mut values[width..2 * width]);
        cols.same_tc.populate(BabyBear::ZERO, BabyBear::ZERO);
        cols.r_limb0_iz
            .populate(BabyBear::new((100u64 & 0x3FFFFFFF) as u32));
        cols.r_limb1_iz.populate(BabyBear::ZERO);
        cols.r_limb2_iz.populate(BabyBear::ZERO);
    }

    RowMajorMatrix::new(values, width)
}
