//! Cross-chip LogUp bus integration tests.
//!
//! Tests that send/receive interactions balance across chip pairs.
//!
//! - C9  StaticTableLookup: Execution → StaticTable (tested below)

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_chips::execution::air::ExecutionChip;
use tabula_chips::execution::trace::generate_execution_trace;
use tabula_chips::static_table::air::StaticTableChip;
use tabula_chips::static_table::trace::{StaticTableRow, generate_static_table_trace};
use tabula_stark::air::interaction::core_buses;
use tabula_stark::debug::{check_bus_balance, evaluate_chip};

use tabula_chips::test_utils::builders::make_lookup;

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
