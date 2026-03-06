//! Tests for permutation trace generation and challenge derivation.

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;
use p3_matrix::dense::RowMajorMatrix;

use tabula_stark::air::descriptor::InteractionDescriptor;
use tabula_stark::air::interaction::{
    Interaction, InteractionDirection, VirtualPairCol, core_buses,
};
use tabula_stark::debug::RecordedInteraction;

use super::challenges::{ChipTraceInfo, derive_challenges_from_main};
use super::trace::{compute_fingerprint_ef4, concat_traces};
use crate::config::EF4;

/// Generate a permutation trace from an interaction descriptor (test-only).
fn generate_permutation_trace(
    main_trace: &RowMajorMatrix<BabyBear>,
    descriptor: &InteractionDescriptor<BabyBear>,
    challenges: [EF4; 2],
) -> (RowMajorMatrix<BabyBear>, EF4) {
    use p3_field::BasedVectorSpace;

    let [alpha, beta] = challenges;
    let height = main_trace.height();

    let num_interactions = descriptor.num_sends_per_row + descriptor.num_receives_per_row;
    assert!(
        num_interactions > 0,
        "cannot generate perm trace for chip with no interactions"
    );

    let perm_width = 4 * (num_interactions + 1);
    let mut perm_values = vec![BabyBear::ZERO; height * perm_width];

    let all_interactions: Vec<&Interaction<BabyBear>> = descriptor
        .sends
        .iter()
        .chain(descriptor.receives.iter())
        .collect();
    assert_eq!(all_interactions.len(), num_interactions);

    let mut cumsum = EF4::ZERO;

    for row_idx in 0..height {
        let local_opt = main_trace.row_slice(row_idx);
        let local = local_opt.as_deref().expect("row must exist");
        let next_row_idx = (row_idx + 1) % height;
        let next_opt = main_trace.row_slice(next_row_idx);
        let next = next_opt.as_deref().expect("next row must exist");

        let perm_row_start = row_idx * perm_width;
        let perm_row = &mut perm_values[perm_row_start..perm_row_start + perm_width];

        for (j, interaction) in all_interactions.iter().enumerate() {
            let mult = interaction.multiplicity.eval(local, next);
            if mult == BabyBear::ZERO {
                continue;
            }
            let values: Vec<BabyBear> = interaction
                .values
                .iter()
                .map(|vpc| vpc.eval(local, next))
                .collect();
            let fingerprint = compute_fingerprint_ef4(&values, interaction.bus, alpha, beta);
            if fingerprint == EF4::ZERO {
                continue;
            }
            let phi = EF4::from(mult) / fingerprint;
            let coeffs = phi.as_basis_coefficients_slice();
            perm_row[j * 4] = coeffs[0];
            perm_row[j * 4 + 1] = coeffs[1];
            perm_row[j * 4 + 2] = coeffs[2];
            perm_row[j * 4 + 3] = coeffs[3];
            match interaction.direction {
                InteractionDirection::Send => cumsum += phi,
                InteractionDirection::Receive => cumsum -= phi,
            }
        }

        let coeffs = cumsum.as_basis_coefficients_slice();
        perm_row[num_interactions * 4] = coeffs[0];
        perm_row[num_interactions * 4 + 1] = coeffs[1];
        perm_row[num_interactions * 4 + 2] = coeffs[2];
        perm_row[num_interactions * 4 + 3] = coeffs[3];
    }

    (RowMajorMatrix::new(perm_values, perm_width), cumsum)
}

fn bb(x: u64) -> BabyBear {
    BabyBear::from_u64(x)
}

#[test]
fn fingerprint_ef4_deterministic() {
    let alpha: EF4 = EF4::from(bb(100));
    let beta: EF4 = EF4::from(bb(200));
    let values = [bb(1), bb(2), bb(3)];

    let f1 = compute_fingerprint_ef4(&values, core_buses::READ_ACCESS, alpha, beta);
    let f2 = compute_fingerprint_ef4(&values, core_buses::READ_ACCESS, alpha, beta);
    assert_eq!(f1, f2);
}

#[test]
fn fingerprint_ef4_different_bus() {
    let alpha: EF4 = EF4::from(bb(100));
    let beta: EF4 = EF4::from(bb(200));
    let values = [bb(1), bb(2), bb(3)];

    let f1 = compute_fingerprint_ef4(&values, core_buses::READ_ACCESS, alpha, beta);
    let f2 = compute_fingerprint_ef4(&values, core_buses::RANGE_CHECK, alpha, beta);
    assert_ne!(f1, f2);
}

/// Compute a single chip's cumsum from debug recorded interactions.
fn debug_chip_cumsum(
    interactions: &[RecordedInteraction<BabyBear>],
    alpha: EF4,
    beta: EF4,
) -> EF4 {
    let mut sum = EF4::ZERO;
    for i in interactions {
        if i.multiplicity == BabyBear::ZERO {
            continue;
        }
        let fp = compute_fingerprint_ef4(&i.values, i.bus, alpha, beta);
        if fp == EF4::ZERO {
            continue;
        }
        let contrib = EF4::from(i.multiplicity) / fp;
        match i.direction {
            InteractionDirection::Send => sum += contrib,
            InteractionDirection::Receive => sum -= contrib,
        }
    }
    sum
}

#[test]
fn balanced_cumsums_sum_to_zero() {
    let [alpha, beta] = [EF4::from(bb(12345)), EF4::from(bb(67890))];

    let send_interactions = vec![RecordedInteraction {
        bus: core_buses::READ_ACCESS,
        values: vec![bb(42)],
        multiplicity: bb(1),
        direction: InteractionDirection::Send,
    }];
    let recv_interactions = vec![RecordedInteraction {
        bus: core_buses::READ_ACCESS,
        values: vec![bb(42)],
        multiplicity: bb(1),
        direction: InteractionDirection::Receive,
    }];

    let cs1 = debug_chip_cumsum(&send_interactions, alpha, beta);
    let cs2 = debug_chip_cumsum(&recv_interactions, alpha, beta);
    assert_eq!(cs1 + cs2, EF4::ZERO);
}

#[test]
fn imbalanced_cumsums_nonzero() {
    let [alpha, beta] = [EF4::from(bb(12345)), EF4::from(bb(67890))];

    let interactions = vec![RecordedInteraction {
        bus: core_buses::READ_ACCESS,
        values: vec![bb(42)],
        multiplicity: bb(1),
        direction: InteractionDirection::Send,
    }];

    let cs = debug_chip_cumsum(&interactions, alpha, beta);
    assert_ne!(cs, EF4::ZERO);
}

#[test]
fn derive_challenges_deterministic() {
    let infos = vec![ChipTraceInfo {
        trace_height: 4,
        public_values: vec![bb(1), bb(2)],
    }];

    let c1 = derive_challenges_from_main(&infos);
    let c2 = derive_challenges_from_main(&infos);
    assert_eq!(c1, c2);
}

#[test]
fn derive_challenges_different_inputs() {
    let infos_a = vec![ChipTraceInfo {
        trace_height: 4,
        public_values: vec![bb(1), bb(2)],
    }];
    let infos_b = vec![ChipTraceInfo {
        trace_height: 8,
        public_values: vec![bb(1), bb(2)],
    }];

    let c1 = derive_challenges_from_main(&infos_a);
    let c2 = derive_challenges_from_main(&infos_b);
    assert_ne!(c1, c2);
}

/// Test: generate permutation trace for a balanced send/receive pair.
#[test]
fn perm_trace_balanced_pair_sums_to_zero() {
    let challenges = [EF4::from(bb(12345)), EF4::from(bb(67890))];

    let sender_trace = RowMajorMatrix::new(
        vec![
            bb(42),
            bb(1),
            bb(99),
            bb(1),
            bb(0),
            bb(0),
            bb(0),
            bb(0),
        ],
        2,
    );

    let send_descriptor = InteractionDescriptor {
        sends: vec![Interaction {
            values: vec![VirtualPairCol::single_local(0)],
            multiplicity: VirtualPairCol::single_local(1),
            bus: core_buses::READ_ACCESS,
            direction: InteractionDirection::Send,
        }],
        receives: vec![],
        num_sends_per_row: 1,
        num_receives_per_row: 0,
    };

    let (send_perm, send_cumsum) =
        generate_permutation_trace(&sender_trace, &send_descriptor, challenges);
    assert_eq!(send_perm.width(), 8);

    let recv_descriptor = InteractionDescriptor {
        sends: vec![],
        receives: vec![Interaction {
            values: vec![VirtualPairCol::single_local(0)],
            multiplicity: VirtualPairCol::single_local(1),
            bus: core_buses::READ_ACCESS,
            direction: InteractionDirection::Receive,
        }],
        num_sends_per_row: 0,
        num_receives_per_row: 1,
    };

    let (recv_perm, recv_cumsum) =
        generate_permutation_trace(&sender_trace, &recv_descriptor, challenges);
    assert_eq!(recv_perm.width(), 8);

    assert_eq!(
        send_cumsum + recv_cumsum,
        EF4::ZERO,
        "balanced send/receive cumsums must sum to zero"
    );
}

/// Test: generate perm trace with zero-multiplicity rows (padding).
#[test]
fn perm_trace_zero_mult_rows_contribute_nothing() {
    let challenges = [EF4::from(bb(111)), EF4::from(bb(222))];

    let trace = RowMajorMatrix::new(
        vec![
            bb(42),
            bb(1),
            bb(0),
            bb(0),
            bb(0),
            bb(0),
            bb(0),
            bb(0),
        ],
        2,
    );

    let descriptor = InteractionDescriptor {
        sends: vec![Interaction {
            values: vec![VirtualPairCol::single_local(0)],
            multiplicity: VirtualPairCol::single_local(1),
            bus: core_buses::READ_ACCESS,
            direction: InteractionDirection::Send,
        }],
        receives: vec![],
        num_sends_per_row: 1,
        num_receives_per_row: 0,
    };

    let (perm, cumsum) = generate_permutation_trace(&trace, &descriptor, challenges);

    let perm_w = perm.width(); // 8
    let cumsum_offset = 4;
    let row1 = perm.row_slice(1).unwrap();
    let row2 = perm.row_slice(2).unwrap();
    let row3 = perm.row_slice(3).unwrap();
    let row1_cumsum = &row1[cumsum_offset..cumsum_offset + 4];
    let row2_cumsum = &row2[cumsum_offset..cumsum_offset + 4];
    let row3_cumsum = &row3[cumsum_offset..cumsum_offset + 4];
    assert_eq!(row1_cumsum, row2_cumsum);
    assert_eq!(row2_cumsum, row3_cumsum);

    assert_ne!(cumsum, EF4::ZERO);
    let _ = perm_w;
}

#[test]
fn concat_traces_correct_layout() {
    let main = RowMajorMatrix::new(vec![bb(1), bb(2), bb(3), bb(4)], 2);
    let perm = RowMajorMatrix::new(vec![bb(10), bb(11), bb(12), bb(20), bb(21), bb(22)], 3);

    let combined = concat_traces(&main, &perm);
    assert_eq!(combined.width(), 5);
    assert_eq!(combined.height(), 2);

    let row0: Vec<BabyBear> = combined.row_slice(0).unwrap().to_vec();
    assert_eq!(row0, vec![bb(1), bb(2), bb(10), bb(11), bb(12)]);

    let row1: Vec<BabyBear> = combined.row_slice(1).unwrap().to_vec();
    assert_eq!(row1, vec![bb(3), bb(4), bb(20), bb(21), bb(22)]);
}
