//! Per-chip quotient polynomial computation for the batched prover.
//!
//! Supports two modes:
//! - **Standard**: chips without interactions (single-phase constraint folding)
//! - **RAP**: chips with interactions (two-phase: inner chip + permutation constraints)

use p3_air::{Air, RowWindow};
use p3_commit::PolynomialSpace;
use p3_field::{BasedVectorSpace, PackedFieldExtension, PackedValue, PrimeCharacteristicRing};
use p3_koala_bear::KoalaBear;
use p3_matrix::Matrix;
use p3_matrix::dense::{RowMajorMatrix, RowMajorMatrixView};
use p3_uni_stark::{PackedVal, ProverConstraintFolder};

use tabula_stark::rap::ef4::build_alpha_powers;
use tabula_stark::rap::prover::RapProverFolder;

use crate::config::{EF4, PcsDomain, TabulaStarkConfig};
use crate::proof::chip_ref::ChipRef;

/// Per-chip info for quotient computation.
///
/// Groups the four scalar parameters that `compute_quotient_rap` needs
/// per chip, reducing the function's argument count.
pub(crate) struct ChipQuotientInfo {
    pub main_width: usize,
    pub inner_constraint_count: usize,
    pub total_constraint_count: usize,
    pub cumsum_final: EF4,
}

/// Decompose EF4 alpha powers into per-dimension base-field coefficient vectors.
///
/// For SIMD-friendly constraint folding: given `[alpha^{k}, alpha^{k-1}, ...]`,
/// returns `D` vectors where `result[d][i]` is the d-th basis coefficient of `powers[i]`.
fn decompose_alpha_powers(powers: &[EF4]) -> Vec<Vec<KoalaBear>> {
    (0..<EF4 as BasedVectorSpace<KoalaBear>>::DIMENSION)
        .map(|d| {
            powers
                .iter()
                .map(|x| x.as_basis_coefficients_slice()[d])
                .collect()
        })
        .collect()
}

/// Build decomposed alpha powers for base-field-only constraints.
///
/// All chip constraints are emitted in the base field, so we construct
/// the decomposition directly: for each basis dimension d, the d-th
/// coordinate of alpha^{N-1-i} for each constraint i.
fn build_base_alpha_powers(alpha: EF4, count: usize) -> (Vec<Vec<KoalaBear>>, Vec<EF4>) {
    let mut alpha_powers: Vec<EF4> = alpha.powers().collect_n(count);
    alpha_powers.reverse();

    let base_alpha_powers = decompose_alpha_powers(&alpha_powers);

    // No extension-field constraints from the AIR.
    let ext_alpha_powers: Vec<EF4> = Vec::new();

    (base_alpha_powers, ext_alpha_powers)
}

/// Build the preprocessed view + window from an optional preprocessed matrix.
///
/// Returns `(view, window)` where `view` is zero-width if no preprocessed
/// trace exists, and `window` wraps the view's two rows.
fn build_preprocessed_view<'a>(
    preprocessed: &'a Option<RowMajorMatrix<PackedVal<TabulaStarkConfig>>>,
) -> (
    RowMajorMatrixView<'a, PackedVal<TabulaStarkConfig>>,
    RowWindow<'a, PackedVal<TabulaStarkConfig>>,
) {
    let view = preprocessed.as_ref().map_or_else(
        || RowMajorMatrixView::new(&[], 0),
        |m: &RowMajorMatrix<_>| m.as_view(),
    );
    let window = RowWindow::from_view(&view);
    (view, window)
}

/// Compute quotient values for a chip without LogUp interactions.
///
/// Uses a single-phase `ProverConstraintFolder` over the full main trace.
#[allow(clippy::too_many_arguments)]
pub(crate) fn compute_quotient_standard<M: Matrix<KoalaBear> + Sync>(
    air: &ChipRef<'_>,
    main_on_q: &M,
    pp_on_q: Option<&M>,
    public_values: &[KoalaBear],
    trace_domain: PcsDomain,
    quotient_domain: PcsDomain,
    alpha: EF4,
    constraint_count: usize,
) -> Vec<EF4> {
    let quotient_size = quotient_domain.size();
    let main_width = main_on_q.width();

    let qdb =
        quotient_size.trailing_zeros() as usize - trace_domain.size().trailing_zeros() as usize;
    let next_step = 1 << qdb;

    let mut sels = trace_domain.selectors_on_coset(quotient_domain);

    type PV = PackedVal<TabulaStarkConfig>;
    for _ in quotient_size..PV::WIDTH {
        sels.is_first_row.push(KoalaBear::default());
        sels.is_last_row.push(KoalaBear::default());
        sels.is_transition.push(KoalaBear::default());
        sels.inv_vanishing.push(KoalaBear::default());
    }

    let (base_alpha_powers, ext_alpha_powers) = build_base_alpha_powers(alpha, constraint_count);

    let mut result = Vec::with_capacity(quotient_size);
    let mut i_start = 0;

    while i_start < quotient_size {
        let i_range = i_start..i_start + PV::WIDTH;

        let is_first_row = *PV::from_slice(&sels.is_first_row[i_range.clone()]);
        let is_last_row = *PV::from_slice(&sels.is_last_row[i_range.clone()]);
        let is_transition = *PV::from_slice(&sels.is_transition[i_range.clone()]);
        let inv_vanishing = *PV::from_slice(&sels.inv_vanishing[i_range]);

        let main_data = main_on_q.vertically_packed_row_pair(i_start, next_step);
        let main_mat = RowMajorMatrix::new(main_data, main_width);

        let preprocessed = pp_on_q.map(|pp| {
            let pp_width = pp.width();
            RowMajorMatrix::new(pp.vertically_packed_row_pair(i_start, next_step), pp_width)
        });
        let (pp_view, pp_window) = build_preprocessed_view(&preprocessed);

        let mut folder = ProverConstraintFolder {
            main: main_mat.as_view(),
            preprocessed: pp_view,
            preprocessed_window: pp_window,
            public_values,
            is_first_row,
            is_last_row,
            is_transition,
            base_alpha_powers: &base_alpha_powers,
            ext_alpha_powers: &ext_alpha_powers,
            base_constraints: Vec::with_capacity(constraint_count),
            ext_constraints: Vec::new(),
            constraint_index: 0,
            constraint_count,
        };
        air.eval(&mut folder);

        let quotient = folder.finalize_constraints() * inv_vanishing;
        let batch_size = std::cmp::min(quotient_size - i_start, PV::WIDTH);
        for idx in 0..batch_size {
            result.push(quotient.extract(idx));
        }

        i_start += PV::WIDTH;
    }

    result
}

/// Compute quotient values for a chip with LogUp interactions.
///
/// Uses two-phase evaluation:
/// - Phase 1: `ProverConstraintFolder` on truncated (main-only) view
/// - Phase 2: `RapProverFolder` on full (main || perm) view
#[allow(clippy::too_many_arguments)]
pub(crate) fn compute_quotient_rap<M: Matrix<KoalaBear> + Sync>(
    air: &ChipRef<'_>,
    main_on_q: &M,
    perm_on_q: &M,
    pp_on_q: Option<&M>,
    public_values: &[KoalaBear],
    trace_domain: PcsDomain,
    quotient_domain: PcsDomain,
    alpha: EF4,
    logup_challenges: [EF4; 2],
    chip_info: &ChipQuotientInfo,
) -> Vec<EF4> {
    let quotient_size = quotient_domain.size();
    let main_width = chip_info.main_width;
    let inner_count = chip_info.inner_constraint_count;
    let total_count = chip_info.total_constraint_count;
    let cumsum_final = chip_info.cumsum_final;
    let perm_width = perm_on_q.width();
    let combined_width = main_width + perm_width;

    let qdb =
        quotient_size.trailing_zeros() as usize - trace_domain.size().trailing_zeros() as usize;
    let next_step = 1 << qdb;

    let mut sels = trace_domain.selectors_on_coset(quotient_domain);

    type PV = PackedVal<TabulaStarkConfig>;
    for _ in quotient_size..PV::WIDTH {
        sels.is_first_row.push(KoalaBear::default());
        sels.is_last_row.push(KoalaBear::default());
        sels.is_transition.push(KoalaBear::default());
        sels.inv_vanishing.push(KoalaBear::default());
    }

    // Full alpha powers for all N constraints (Phase 1 + Phase 2).
    // Phase 1 uses the first M entries (highest powers: alpha^{N-1}, ..., alpha^{N-M}).
    // Phase 2 uses entries M..N (lower powers: alpha^{N-M-1}, ..., 1).
    let (rap_alpha_powers, _) = build_alpha_powers(alpha, total_count);

    // Build decomposed base-field powers for Phase 1 from the first inner_count entries.
    let base_alpha_powers = decompose_alpha_powers(&rap_alpha_powers[..inner_count]);
    let ext_alpha_powers_inner: Vec<EF4> = Vec::new();

    let mut result = Vec::with_capacity(quotient_size);
    let mut i_start = 0;

    while i_start < quotient_size {
        let i_range = i_start..i_start + PV::WIDTH;

        let is_first_row = *PV::from_slice(&sels.is_first_row[i_range.clone()]);
        let is_last_row = *PV::from_slice(&sels.is_last_row[i_range.clone()]);
        let is_transition = *PV::from_slice(&sels.is_transition[i_range.clone()]);
        let inv_vanishing = *PV::from_slice(&sels.inv_vanishing[i_range]);

        // Get packed rows from separate main and perm matrices
        let main_packed = main_on_q.vertically_packed_row_pair(i_start, next_step);
        let perm_packed = perm_on_q.vertically_packed_row_pair(i_start, next_step);

        // Truncated view: main columns only
        let truncated_main = RowMajorMatrix::new(main_packed.clone(), main_width);

        // Full view: main || perm (concatenate local parts, then next parts)
        let mut full_data = Vec::with_capacity(2 * combined_width);
        full_data.extend_from_slice(&main_packed[..main_width]);
        full_data.extend_from_slice(&perm_packed[..perm_width]);
        full_data.extend_from_slice(&main_packed[main_width..]);
        full_data.extend_from_slice(&perm_packed[perm_width..]);
        let full_main = RowMajorMatrix::new(full_data, combined_width);

        let preprocessed = pp_on_q.map(|pp| {
            let pp_width = pp.width();
            RowMajorMatrix::new(pp.vertically_packed_row_pair(i_start, next_step), pp_width)
        });
        let (pp_view, pp_window) = build_preprocessed_view(&preprocessed);

        // Phase 1: Inner chip constraints (truncated view)
        let mut folder1 = ProverConstraintFolder {
            main: truncated_main.as_view(),
            preprocessed: pp_view,
            preprocessed_window: pp_window,
            public_values,
            is_first_row,
            is_last_row,
            is_transition,
            base_alpha_powers: &base_alpha_powers,
            ext_alpha_powers: &ext_alpha_powers_inner,
            base_constraints: Vec::with_capacity(inner_count),
            ext_constraints: Vec::new(),
            constraint_index: 0,
            constraint_count: inner_count,
        };
        air.eval(&mut folder1);

        debug_assert_eq!(
            folder1.constraint_index, inner_count,
            "inner chip produced unexpected constraint count"
        );

        // Finalize Phase 1 to get the accumulated constraint value.
        let phase1_acc = folder1.finalize_constraints();

        // Phase 2: RAP constraints (full view via RapProverFolder)
        let mut rap_folder = RapProverFolder::new(
            truncated_main.as_view(),
            full_main.as_view(),
            preprocessed
                .as_ref()
                .map(|m: &RowMajorMatrix<PackedVal<TabulaStarkConfig>>| m.as_view()),
            public_values,
            is_first_row,
            is_last_row,
            is_transition,
            &rap_alpha_powers,
            phase1_acc,
            inner_count,
            logup_challenges,
            main_width,
        );
        air.eval(&mut rap_folder);

        // Broadcast cumsum_final EF4 into packed values for last-row constraint.
        let cumsum_coeffs = tabula_stark::rap::ef4::ef4_coeffs(cumsum_final);
        let cumsum_final_packed: [PV; 4] = [
            PV::from(cumsum_coeffs[0]),
            PV::from(cumsum_coeffs[1]),
            PV::from(cumsum_coeffs[2]),
            PV::from(cumsum_coeffs[3]),
        ];
        rap_folder.finalize_cumsum(cumsum_final_packed);

        debug_assert_eq!(
            rap_folder.constraint_index(),
            total_count,
            "RAP produced unexpected constraint count (expected {total_count}, got {})",
            rap_folder.constraint_index(),
        );

        let quotient = rap_folder.accumulator() * inv_vanishing;
        let batch_size = std::cmp::min(quotient_size - i_start, PV::WIDTH);
        for idx in 0..batch_size {
            result.push(quotient.extract(idx));
        }

        i_start += PV::WIDTH;
    }

    result
}
