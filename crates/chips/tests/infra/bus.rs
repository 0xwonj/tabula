//! Cross-chip LogUp bus integration tests.
//!
//! Tests that send/receive interactions balance across chip pairs.
//!
//! - C9  StaticTableLookup: Execution → StaticTable (tested below)

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;

use tabula_chips::execution::air::ExecutionChip;
use tabula_chips::execution::trace::generate_execution_trace;
use tabula_chips::static_table::air::StaticTableChip;
use tabula_chips::static_table::trace::{StaticTableRow, generate_static_table_trace};
use tabula_stark::air::interaction::core_buses;
use tabula_stark::debug::{check_bus_balance, evaluate_chip};

use tabula_chips::test_utils::builders::{make_lookup, make_property_read};

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
        value: vec![KoalaBear::new(42), KoalaBear::ZERO, KoalaBear::ZERO],
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

// ── C18 PropertyRead: Execution → SSMC Property ──

use tabula_chips::shards::property::SsmcPropertyChip;
use tabula_chips::test_utils::builders::{SsmcPropertyTestRow, generate_ssmc_property_test_trace};
use tabula_stark::chips::ChipId;

#[test]
fn c18_property_read_bus_balance_single_query() {
    // ExecutionChip sends one PropertyRead successor query on table=5, col=2.
    let result_val = vec![KoalaBear::new(42), KoalaBear::ZERO, KoalaBear::ZERO];
    let result_key = vec![KoalaBear::new(100), KoalaBear::ZERO, KoalaBear::ZERO];

    let exec_rec = make_property_read(
        0,
        1,
        2,
        5,
        2,
        2,
        50,
        0,
        result_val.clone(),
        result_key.clone(),
        false,
    );
    let exec_trace = generate_execution_trace::<3>(&[exec_rec]);
    let exec_record = evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap();

    let property_chip = SsmcPropertyChip::<3>::new(ChipId(100), 5, 2);
    let property_trace = generate_ssmc_property_test_trace::<3>(
        5,
        2,
        &[SsmcPropertyTestRow::Successor {
            query_key: 50,
            anchor_key: 100,
            prev_key: None,
            result_val,
        }],
    );
    let property_record = evaluate_chip("SsmcProperty", &property_chip, &property_trace).unwrap();

    check_bus_balance(&[exec_record, property_record], core_buses::PROPERTY_READ)
        .expect("C18 PropertyRead should balance for single query");
}

#[test]
fn c18_property_read_bus_balance_multiple_queries() {
    // Two PropertyRead queries on the same column.
    let val1 = vec![KoalaBear::new(42), KoalaBear::ZERO, KoalaBear::ZERO];
    let key1 = vec![KoalaBear::new(100), KoalaBear::ZERO, KoalaBear::ZERO];
    let val2 = vec![KoalaBear::new(99), KoalaBear::ZERO, KoalaBear::ZERO];
    let key2 = vec![KoalaBear::new(200), KoalaBear::ZERO, KoalaBear::ZERO];

    let mut exec0 = make_property_read(0, 1, 2, 5, 2, 2, 50, 0, val1.clone(), key1.clone(), false);
    exec0.tx_index = 0;
    let mut exec1 = make_property_read(3, 4, 5, 5, 2, 3, 250, 0, val2.clone(), key2.clone(), false);
    exec1.tx_index = 1;

    let exec_trace = generate_execution_trace::<3>(&[exec0, exec1]);
    let exec_record = evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap();

    let property_chip = SsmcPropertyChip::<3>::new(ChipId(100), 5, 2);
    let property_trace = generate_ssmc_property_test_trace::<3>(
        5,
        2,
        &[
            SsmcPropertyTestRow::Successor {
                query_key: 50,
                anchor_key: 100,
                prev_key: None,
                result_val: val1,
            },
            SsmcPropertyTestRow::Predecessor {
                query_key: 250,
                anchor_key: 200,
                next_key: None,
                result_val: val2,
            },
        ],
    );
    let property_record = evaluate_chip("SsmcProperty", &property_chip, &property_trace).unwrap();

    check_bus_balance(&[exec_record, property_record], core_buses::PROPERTY_READ)
        .expect("C18 PropertyRead should balance for multiple queries");
}

#[test]
fn c18_property_read_bus_balance_null_result() {
    // PropertyRead query returning null.
    let val = vec![KoalaBear::ZERO, KoalaBear::ZERO, KoalaBear::ZERO];
    let key = vec![KoalaBear::ZERO, KoalaBear::ZERO, KoalaBear::ZERO];

    let exec_rec = make_property_read(0, 1, 2, 3, 1, 2, 999, 0, val.clone(), key.clone(), true);
    let exec_trace = generate_execution_trace::<3>(&[exec_rec]);
    let exec_record = evaluate_chip("Execution", &ExecutionChip::<3>, &exec_trace).unwrap();

    let property_chip = SsmcPropertyChip::<3>::new(ChipId(100), 3, 1);
    let property_trace = generate_ssmc_property_test_trace::<3>(
        3,
        1,
        &[SsmcPropertyTestRow::EmptySuccessor { query_key: 999 }],
    );
    let property_record = evaluate_chip("SsmcProperty", &property_chip, &property_trace).unwrap();

    check_bus_balance(&[exec_record, property_record], core_buses::PROPERTY_READ)
        .expect("C18 PropertyRead should balance for null result");
}
