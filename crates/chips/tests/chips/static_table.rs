//! Tests for the StaticTableChip.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_chips::static_table::air::StaticTableChip;
use tabula_chips::static_table::columns::{STATIC_TABLE_STANDARD_WIDTH, static_table_width};
use tabula_chips::static_table::trace::{StaticTableRow, generate_static_table_trace};
use tabula_stark::debug::{debug_check, evaluate_chip};

// ── Width ──

#[test]
fn static_table_standard_width() {
    assert_eq!(STATIC_TABLE_STANDARD_WIDTH, 10);
    assert_eq!(static_table_width::<3>(), 10);
}

// ── Constraint checks ──

#[test]
fn valid_single_row() {
    let rows = vec![StaticTableRow {
        table_id: 1,
        col_id: 0,
        row_key: 100,
        value: vec![BabyBear::new(42), BabyBear::ZERO, BabyBear::ZERO],
        lookup_mult: 1,
    }];
    let trace = generate_static_table_trace::<3>(&rows);
    debug_check(&StaticTableChip::<3>, &trace).expect("single row should pass");
}

#[test]
fn valid_multiple_rows() {
    let rows = vec![
        StaticTableRow {
            table_id: 1,
            col_id: 0,
            row_key: 0,
            value: vec![BabyBear::new(10), BabyBear::ZERO, BabyBear::ZERO],
            lookup_mult: 1,
        },
        StaticTableRow {
            table_id: 1,
            col_id: 0,
            row_key: 1,
            value: vec![BabyBear::new(20), BabyBear::ZERO, BabyBear::ZERO],
            lookup_mult: 1,
        },
        StaticTableRow {
            table_id: 2,
            col_id: 3,
            row_key: 42,
            value: vec![BabyBear::new(99), BabyBear::new(7), BabyBear::ZERO],
            lookup_mult: 1,
        },
    ];
    let trace = generate_static_table_trace::<3>(&rows);
    debug_check(&StaticTableChip::<3>, &trace).expect("multiple rows should pass");
}

#[test]
fn valid_empty_trace() {
    let trace = generate_static_table_trace::<3>(&[]);
    debug_check(&StaticTableChip::<3>, &trace).expect("empty trace should pass");
}

// ── Interaction recording ──

#[test]
fn records_c9_receive_interactions() {
    let rows = vec![StaticTableRow {
        table_id: 1,
        col_id: 0,
        row_key: 100,
        value: vec![BabyBear::new(42), BabyBear::ZERO, BabyBear::ZERO],
        lookup_mult: 1,
    }];
    let trace = generate_static_table_trace::<3>(&rows);
    let record = evaluate_chip("StaticTable", &StaticTableChip::<3>, &trace).unwrap();

    // Count non-zero multiplicity receives
    use tabula_stark::air::interaction::{InteractionDirection, core_buses};
    let c9_receives: Vec<_> = record
        .interactions
        .iter()
        .filter(|i| {
            i.bus == core_buses::STATIC_TABLE_LOOKUP
                && i.direction == InteractionDirection::Receive
                && i.multiplicity != BabyBear::ZERO
        })
        .collect();
    assert_eq!(c9_receives.len(), 1, "should have exactly 1 C9 receive");
}

#[test]
fn c9_receive_uses_lookup_multiplicity_witness() {
    let rows = vec![StaticTableRow {
        table_id: 1,
        col_id: 0,
        row_key: 100,
        value: vec![BabyBear::new(42), BabyBear::ZERO, BabyBear::ZERO],
        lookup_mult: 3,
    }];
    let trace = generate_static_table_trace::<3>(&rows);
    let record = evaluate_chip("StaticTable", &StaticTableChip::<3>, &trace).unwrap();

    use tabula_stark::air::interaction::{InteractionDirection, core_buses};
    let c9_receives: Vec<_> = record
        .interactions
        .iter()
        .filter(|i| {
            i.bus == core_buses::STATIC_TABLE_LOOKUP
                && i.direction == InteractionDirection::Receive
                && i.multiplicity != BabyBear::ZERO
        })
        .collect();
    assert_eq!(
        c9_receives.len(),
        1,
        "one tuple receive at non-zero multiplicity"
    );
    assert_eq!(
        c9_receives[0].multiplicity,
        BabyBear::new(3),
        "receive multiplicity should match lookup_mult witness"
    );
}
