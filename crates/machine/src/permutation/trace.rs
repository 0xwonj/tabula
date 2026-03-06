//! Permutation trace generation and EF4 fingerprint computation.

use p3_baby_bear::BabyBear;
use p3_field::{BasedVectorSpace, PrimeCharacteristicRing};
use p3_matrix::Matrix;
use p3_matrix::dense::RowMajorMatrix;

use tabula_stark::air::interaction::{BusId, InteractionDirection};
use tabula_stark::debug::RecordedInteraction;

use crate::config::EF4;

/// Compute an RLC fingerprint in the extension field EF4.
///
/// `f = α + kind_tag + β · values[0] + β² · values[1] + …`
pub(crate) fn compute_fingerprint_ef4(
    values: &[BabyBear],
    bus: BusId,
    alpha: EF4,
    beta: EF4,
) -> EF4 {
    let mut result = alpha + EF4::from(BabyBear::from_u64(bus.tag() as u64));
    let mut beta_power = beta;
    for &val in values {
        result += beta_power * EF4::from(val);
        beta_power *= beta;
    }
    result
}

/// Write an EF4 value into a trace row at the given column offset.
fn write_ef4(row: &mut [BabyBear], offset: usize, val: EF4) {
    let coeffs = val.as_basis_coefficients_slice();
    row[offset] = coeffs[0];
    row[offset + 1] = coeffs[1];
    row[offset + 2] = coeffs[2];
    row[offset + 3] = coeffs[3];
}

/// Generate a permutation trace from concrete recorded interactions.
///
/// Uses concrete interaction values recorded by evaluating the chip's `eval()`
/// via [`evaluate_chip_interactions_only`]. This correctly handles non-affine
/// multiplicities (products of columns).
///
/// # Trace layout
///
/// ```text
/// | phi_0 (4 cols) | ... | phi_{N-1} (4 cols) | cumsum (4 cols) |
/// ```
///
/// # Returns
///
/// `(permutation_trace, cumsum_final)`
pub(crate) fn generate_permutation_trace_from_interactions(
    recorded: &[RecordedInteraction<BabyBear>],
    height: usize,
    challenges: [EF4; 2],
) -> (RowMajorMatrix<BabyBear>, EF4) {
    let [alpha, beta] = challenges;

    assert!(
        !recorded.is_empty(),
        "cannot generate perm trace with no interactions"
    );
    assert_eq!(
        recorded.len() % height,
        0,
        "interaction count {} is not divisible by height {}",
        recorded.len(),
        height
    );

    let interactions_per_row = recorded.len() / height;
    let perm_width = 4 * (interactions_per_row + 1); // N phis + 1 cumsum
    let mut perm_values = vec![BabyBear::ZERO; height * perm_width];

    let mut cumsum = EF4::ZERO;

    for row_idx in 0..height {
        let perm_row_start = row_idx * perm_width;
        let perm_row = &mut perm_values[perm_row_start..perm_row_start + perm_width];

        let row_start = row_idx * interactions_per_row;
        let row_interactions = &recorded[row_start..row_start + interactions_per_row];

        for (j, interaction) in row_interactions.iter().enumerate() {
            let mult = interaction.multiplicity;

            if mult == BabyBear::ZERO {
                // phi = 0 when multiplicity is zero. Already zero-initialized.
                continue;
            }

            let fingerprint =
                compute_fingerprint_ef4(&interaction.values, interaction.bus, alpha, beta);

            if fingerprint == EF4::ZERO {
                // Negligible probability (~2^{-124}). Skip to avoid division by zero.
                continue;
            }

            let phi = EF4::from(mult) / fingerprint;

            // Write phi into perm trace.
            write_ef4(perm_row, j * 4, phi);

            // Accumulate into running cumsum.
            match interaction.direction {
                InteractionDirection::Send => cumsum += phi,
                InteractionDirection::Receive => cumsum -= phi,
            }
        }

        // Write cumsum at the end of the row.
        write_ef4(perm_row, interactions_per_row * 4, cumsum);
    }

    let perm_trace = RowMajorMatrix::new(perm_values, perm_width);
    (perm_trace, cumsum)
}

/// Horizontally concatenate main and permutation traces.
///
/// Returns a new trace with width = `main_width + perm_width`.
pub(crate) fn concat_traces(
    main_trace: &RowMajorMatrix<BabyBear>,
    perm_trace: &RowMajorMatrix<BabyBear>,
) -> RowMajorMatrix<BabyBear> {
    let height = main_trace.height();
    assert_eq!(height, perm_trace.height(), "trace heights must match");

    let main_w = main_trace.width();
    let perm_w = perm_trace.width();
    let combined_w = main_w + perm_w;

    let mut values = vec![BabyBear::ZERO; height * combined_w];
    for row in 0..height {
        let main_row = main_trace.row_slice(row).expect("row must exist");
        let perm_row = perm_trace.row_slice(row).expect("row must exist");
        let dst_start = row * combined_w;
        values[dst_start..dst_start + main_w].copy_from_slice(&main_row);
        values[dst_start + main_w..dst_start + combined_w].copy_from_slice(&perm_row);
    }

    RowMajorMatrix::new(values, combined_w)
}
