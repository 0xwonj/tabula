use super::*;
use p3_air::{Air, BaseAir, WindowAccess};
use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use crate::air::builder::InteractionAirBuilder;
use crate::air::interaction::{AirInteraction, InteractionDirection, core_buses};

/// Minimal chip that sends one interaction per real row.
#[derive(Debug)]
struct SenderChip;

impl<F> BaseAir<F> for SenderChip {
    fn width(&self) -> usize {
        2 // [is_real, value]
    }
}

impl<AB: InteractionAirBuilder> Air<AB> for SenderChip {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local = main.current_slice();
        let is_real: AB::Expr = local[0].into();
        let value: AB::Expr = local[1].into();

        builder.send(AirInteraction {
            values: vec![value],
            multiplicity: is_real,
            bus: core_buses::READ_ACCESS,
        });
    }
}

/// Minimal chip that receives one interaction per real row.
#[derive(Debug)]
struct ReceiverChip;

impl<F> BaseAir<F> for ReceiverChip {
    fn width(&self) -> usize {
        2
    }
}

impl<AB: InteractionAirBuilder> Air<AB> for ReceiverChip {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local = main.current_slice();
        let is_real: AB::Expr = local[0].into();
        let value: AB::Expr = local[1].into();

        builder.receive(AirInteraction {
            values: vec![value],
            multiplicity: is_real,
            bus: core_buses::READ_ACCESS,
        });
    }
}

fn bb(x: u32) -> KoalaBear {
    KoalaBear::new(x)
}

fn make_trace(rows: &[[u32; 2]]) -> RowMajorMatrix<KoalaBear> {
    let padded_len = rows.len().next_power_of_two().max(2);
    let mut values = vec![KoalaBear::ZERO; padded_len * 2];
    for (i, row) in rows.iter().enumerate() {
        values[i * 2] = bb(row[0]);
        values[i * 2 + 1] = bb(row[1]);
    }
    RowMajorMatrix::new(values, 2)
}

/// Helper: evaluate heterogeneous chips and check LogUp balance.
#[allow(clippy::needless_pass_by_value)]
fn assert_logup_balanced(records: Vec<ChipRecord<KoalaBear>>) {
    check_logup_balance(&records).expect("LogUp should balance");
}

/// Helper: evaluate heterogeneous chips and assert LogUp imbalance.
#[allow(clippy::needless_pass_by_value)]
fn assert_logup_imbalanced(records: Vec<ChipRecord<KoalaBear>>) {
    let err = check_logup_balance(&records).unwrap_err();
    assert!(
        matches!(err, MultiChipError::LogUpImbalance { .. }),
        "expected LogUpImbalance, got {err:?}"
    );
}

#[test]
fn logup_balanced_simple() {
    let sender_trace = make_trace(&[[1, 42]]);
    let receiver_trace = make_trace(&[[1, 42]]);

    let records = vec![
        evaluate_chip("Sender", &SenderChip, &sender_trace).unwrap(),
        evaluate_chip("Receiver", &ReceiverChip, &receiver_trace).unwrap(),
    ];
    assert_logup_balanced(records);
}

#[test]
fn logup_imbalanced_missing_receive() {
    let sender_trace = make_trace(&[[1, 42]]);

    let records = vec![evaluate_chip("Sender", &SenderChip, &sender_trace).unwrap()];
    assert_logup_imbalanced(records);
}

#[test]
fn logup_imbalanced_wrong_value() {
    let sender_trace = make_trace(&[[1, 42]]);
    let receiver_trace = make_trace(&[[1, 99]]);

    let records = vec![
        evaluate_chip("Sender", &SenderChip, &sender_trace).unwrap(),
        evaluate_chip("Receiver", &ReceiverChip, &receiver_trace).unwrap(),
    ];
    assert_logup_imbalanced(records);
}

#[test]
fn logup_balanced_multiple_rows() {
    let sender_trace = make_trace(&[[1, 10], [1, 20], [1, 30]]);
    let receiver_trace = make_trace(&[[1, 10], [1, 20], [1, 30]]);

    let records = vec![
        evaluate_chip("Sender", &SenderChip, &sender_trace).unwrap(),
        evaluate_chip("Receiver", &ReceiverChip, &receiver_trace).unwrap(),
    ];
    assert_logup_balanced(records);
}

#[test]
fn logup_zero_multiplicity_ignored() {
    // is_real=0 rows should be ignored.
    let sender_trace = make_trace(&[[0, 42]]);
    let receiver_trace = make_trace(&[]);

    let records = vec![
        evaluate_chip("Sender", &SenderChip, &sender_trace).unwrap(),
        evaluate_chip("Receiver", &ReceiverChip, &receiver_trace).unwrap(),
    ];
    assert_logup_balanced(records);
}

#[test]
fn logup_balanced_multiset_duplicates() {
    let sender_trace = make_trace(&[[1, 42], [1, 42]]);
    let receiver_trace = make_trace(&[[1, 42], [1, 42]]);

    let records = vec![
        evaluate_chip("Sender", &SenderChip, &sender_trace).unwrap(),
        evaluate_chip("Receiver", &ReceiverChip, &receiver_trace).unwrap(),
    ];
    assert_logup_balanced(records);
}

#[test]
fn logup_imbalanced_duplicate_count_mismatch() {
    let sender_trace = make_trace(&[[1, 42], [1, 42]]);
    let receiver_trace = make_trace(&[[1, 42]]);

    let records = vec![
        evaluate_chip("Sender", &SenderChip, &sender_trace).unwrap(),
        evaluate_chip("Receiver", &ReceiverChip, &receiver_trace).unwrap(),
    ];
    assert_logup_imbalanced(records);
}

#[test]
fn fingerprint_deterministic() {
    let alpha = bb(100);
    let beta = bb(200);
    let values = [bb(1), bb(2), bb(3)];

    let f1 = compute_fingerprint(&values, core_buses::READ_ACCESS, alpha, beta);
    let f2 = compute_fingerprint(&values, core_buses::READ_ACCESS, alpha, beta);
    assert_eq!(f1, f2);

    // Different bus kind produces different fingerprint.
    let f3 = compute_fingerprint(&values, core_buses::RANGE_CHECK, alpha, beta);
    assert_ne!(f1, f3);
}

#[test]
fn fingerprint_different_values() {
    let alpha = bb(100);
    let beta = bb(200);

    let f1 = compute_fingerprint(&[bb(1), bb(2)], core_buses::READ_ACCESS, alpha, beta);
    let f2 = compute_fingerprint(&[bb(1), bb(3)], core_buses::READ_ACCESS, alpha, beta);
    assert_ne!(f1, f2);
}

#[test]
fn single_chip_debug_check_still_works() {
    // Existing single-chip debug_check works with interaction-aware builder.
    let trace = make_trace(&[[1, 42]]);
    debug_check(&SenderChip, &trace).expect("no local constraints to fail");
}

#[test]
fn evaluate_chip_records_interactions() {
    let trace = make_trace(&[[1, 42]]);
    let record = evaluate_chip("Sender", &SenderChip, &trace).unwrap();

    // 2 rows (1 real + 1 padding), real row emits 1 interaction.
    let nonzero: Vec<_> = record
        .interactions
        .iter()
        .filter(|i| i.multiplicity != p3_koala_bear::KoalaBear::ZERO)
        .collect();
    assert_eq!(nonzero.len(), 1);
    assert_eq!(nonzero[0].bus, core_buses::READ_ACCESS);
    assert_eq!(nonzero[0].direction, InteractionDirection::Send);
}
