//! Permutation trace generation and EF4 fingerprint computation.

use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;
use p3_field::{BasedVectorSpace, Field, PrimeCharacteristicRing};
use p3_matrix::dense::RowMajorMatrix;

use crate::EF4;
use crate::air::interaction::{BusId, InteractionDirection};
use crate::debug::RecordedInteraction;

use super::PermutationError;

/// Output of permutation trace generation.
pub struct PermutationTraceOutput {
    /// The permutation trace matrix.
    pub trace: RowMajorMatrix<BabyBear>,
    /// Total cumulative sum across all interactions.
    pub cumsum: EF4,
    /// Per-bus cumulative sums (for cross-proof bus balance).
    pub cumsums_by_bus: BTreeMap<BusId, EF4>,
}

/// Compute an RLC fingerprint in the extension field EF4.
///
/// `f = α + kind_tag + β · values[0] + β² · values[1] + …`
pub fn compute_fingerprint_ef4(values: &[BabyBear], bus: BusId, alpha: EF4, beta: EF4) -> EF4 {
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
/// `Ok(PermutationTraceOutput { trace, cumsum, cumsums_by_bus })` on success.
///
/// # Errors
///
/// Returns [`PermutationError::FingerprintZero`] if a LogUp fingerprint evaluates
/// to zero (probability ~2^{-124} with random challenges).
pub fn generate_permutation_trace_from_interactions(
    recorded: &[RecordedInteraction<BabyBear>],
    height: usize,
    challenges: [EF4; 2],
) -> Result<PermutationTraceOutput, PermutationError> {
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

    // ── Pass 1: Compute fingerprints for non-zero interactions ──────────
    let mut multiplicities: Vec<BabyBear> = Vec::new();
    let mut fingerprints: Vec<EF4> = Vec::new();

    for row_idx in 0..height {
        let row_start = row_idx * interactions_per_row;
        let row_interactions = &recorded[row_start..row_start + interactions_per_row];

        for (j, interaction) in row_interactions.iter().enumerate() {
            if interaction.multiplicity == BabyBear::ZERO {
                continue;
            }

            let fp = compute_fingerprint_ef4(&interaction.values, interaction.bus, alpha, beta);
            if fp == EF4::ZERO {
                return Err(PermutationError::FingerprintZero {
                    row: row_idx,
                    interaction: j,
                });
            }

            fingerprints.push(fp);
            multiplicities.push(interaction.multiplicity);
        }
    }

    // ── Montgomery batch inversion ──────────────────────────────────────
    let inverses = batch_inverse_ef4(&fingerprints);

    // ── Pass 2: Write phi values and accumulate cumsums (order-preserving) ─
    let mut cumsum = EF4::ZERO;
    let mut cumsums_by_bus: BTreeMap<BusId, EF4> = BTreeMap::new();
    let mut cursor = 0;

    for row_idx in 0..height {
        let perm_row_start = row_idx * perm_width;
        let perm_row = &mut perm_values[perm_row_start..perm_row_start + perm_width];

        let row_start = row_idx * interactions_per_row;
        let row_interactions = &recorded[row_start..row_start + interactions_per_row];

        for (j, interaction) in row_interactions.iter().enumerate() {
            if interaction.multiplicity == BabyBear::ZERO {
                continue;
            }

            let phi = EF4::from(multiplicities[cursor]) * inverses[cursor];
            cursor += 1;

            write_ef4(perm_row, j * 4, phi);

            let bus_entry = cumsums_by_bus.entry(interaction.bus).or_insert(EF4::ZERO);
            match interaction.direction {
                InteractionDirection::Send => {
                    cumsum += phi;
                    *bus_entry += phi;
                }
                InteractionDirection::Receive => {
                    cumsum -= phi;
                    *bus_entry -= phi;
                }
            }
        }

        write_ef4(perm_row, interactions_per_row * 4, cumsum);
    }

    let trace = RowMajorMatrix::new(perm_values, perm_width);
    Ok(PermutationTraceOutput {
        trace,
        cumsum,
        cumsums_by_bus,
    })
}

/// Montgomery batch inversion: invert N field elements with 1 inversion + 3(N-1) multiplications.
///
/// Returns `inverses[i] = elements[i]^{-1}` for all i.
/// Assumes no element is zero (caller must check beforehand).
fn batch_inverse_ef4(elements: &[EF4]) -> Vec<EF4> {
    if elements.is_empty() {
        return vec![];
    }
    if elements.len() == 1 {
        return vec![elements[0].inverse()];
    }

    // Prefix products: prefix[i] = elements[0] * elements[1] * ... * elements[i]
    let mut prefix = Vec::with_capacity(elements.len());
    prefix.push(elements[0]);
    for i in 1..elements.len() {
        prefix.push(prefix[i - 1] * elements[i]);
    }

    // Invert the total product
    let mut inv_acc = prefix[elements.len() - 1].inverse();

    // Backtrack to recover individual inverses
    let mut inverses = vec![EF4::ZERO; elements.len()];
    for i in (1..elements.len()).rev() {
        inverses[i] = inv_acc * prefix[i - 1];
        inv_acc *= elements[i];
    }
    inverses[0] = inv_acc;

    inverses
}

/// Horizontally concatenate main and permutation traces (test only).
///
/// Returns a new trace with width = `main_width + perm_width`.
#[cfg(test)]
pub(crate) fn concat_traces(
    main_trace: &RowMajorMatrix<BabyBear>,
    perm_trace: &RowMajorMatrix<BabyBear>,
) -> RowMajorMatrix<BabyBear> {
    use p3_matrix::Matrix;
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
