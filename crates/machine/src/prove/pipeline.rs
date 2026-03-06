//! Per-chip proving pipeline: standard and RAP (two-phase) variants.

use p3_air::Air;
use p3_baby_bear::BabyBear;
use p3_challenger::{CanObserve, FieldChallenger};
use p3_commit::{Pcs, PolynomialSpace};
use p3_field::{BasedVectorSpace, PackedFieldExtension, PackedValue, PrimeCharacteristicRing};
use p3_matrix::Matrix;
use p3_matrix::dense::RowMajorMatrix;
use p3_uni_stark::{
    Commitments, OpenedValues, PackedChallenge, PackedVal, Proof, ProverConstraintFolder,
    StarkGenericConfig, get_log_num_quotient_chunks, get_symbolic_constraints, setup_preprocessed,
};

use crate::chip_ref::ChipRef;
use crate::config::{Challenger, EF4, TabulaPcs, TabulaStarkConfig};
use crate::proof::ChipProofEntry;

use super::RapProverFolder;

// ─── Per-chip provers ───────────────────────────────────────────────────────

/// Prove a chip without interactions using the standard p3-uni-stark pipeline.
pub(super) fn prove_chip_standard(
    config: &TabulaStarkConfig,
    chip_ref: &ChipRef<'_>,
    main_trace: &RowMajorMatrix<BabyBear>,
    public_values: &[BabyBear],
    main_width: usize,
) -> ChipProofEntry {
    let height = main_trace.height();
    let degree_bits = height.trailing_zeros() as usize;

    let pp_setup = setup_preprocessed(config, chip_ref, degree_bits);
    let (pp_prover, pp_vk) = match pp_setup {
        Some((prover, vk)) => (Some(prover), Some(vk)),
        None => (None, None),
    };

    let proof = p3_uni_stark::prove_with_preprocessed(
        config,
        chip_ref,
        main_trace.clone(),
        public_values,
        pp_prover.as_ref(),
    );

    ChipProofEntry {
        chip_id: chip_ref.chip_id(),
        proof,
        cumsum_final: EF4::ZERO,
        trace_height: height,
        main_width,
        perm_width: 0,
        public_values: public_values.to_vec(),
        preprocessed_vk: pp_vk,
    }
}

/// Prove a chip with interactions using the custom two-phase RAP pipeline.
#[allow(clippy::too_many_arguments)]
pub(super) fn prove_chip_rap(
    config: &TabulaStarkConfig,
    chip_ref: &ChipRef<'_>,
    combined_trace: RowMajorMatrix<BabyBear>,
    public_values: &[BabyBear],
    logup_challenges: [EF4; 2],
    main_width: usize,
    perm_width: usize,
    interactions_per_row: usize,
    cumsum: EF4,
) -> ChipProofEntry {
    let height = combined_trace.height();
    let degree_bits = height.trailing_zeros() as usize;

    let pp_setup = setup_preprocessed(config, chip_ref, degree_bits);
    let (pp_prover, pp_vk) = match pp_setup {
        Some((prover, vk)) => (Some(prover), Some(vk)),
        None => (None, None),
    };
    let preprocessed_width = pp_prover.as_ref().map_or(0, |pp| pp.width);

    let inner_count =
        get_symbolic_constraints(chip_ref, preprocessed_width, public_values.len()).len();
    let rap_count = crate::keys::rap_constraint_count(interactions_per_row);
    let total_count = inner_count + rap_count;

    let inner_log =
        get_log_num_quotient_chunks(chip_ref, preprocessed_width, public_values.len(), 0);
    let log_num_quotient_chunks = inner_log.max(2);

    let proof = pcs_commit_and_open(
        config,
        chip_ref,
        combined_trace,
        public_values,
        pp_prover.as_ref(),
        preprocessed_width,
        inner_count,
        total_count,
        log_num_quotient_chunks,
        degree_bits,
        main_width,
        logup_challenges,
    );

    ChipProofEntry {
        chip_id: chip_ref.chip_id(),
        proof,
        cumsum_final: cumsum,
        trace_height: height,
        main_width,
        perm_width,
        public_values: public_values.to_vec(),
        preprocessed_vk: pp_vk,
    }
}

/// PCS ceremony: commit combined trace, observe metadata, sample alpha,
/// compute quotient polynomial, commit quotient, and generate opening proof.
#[allow(clippy::too_many_arguments)]
fn pcs_commit_and_open(
    config: &TabulaStarkConfig,
    chip_ref: &ChipRef<'_>,
    combined_trace: RowMajorMatrix<BabyBear>,
    public_values: &[BabyBear],
    pp_prover: Option<&p3_uni_stark::PreprocessedProverData<TabulaStarkConfig>>,
    preprocessed_width: usize,
    inner_count: usize,
    total_count: usize,
    log_num_quotient_chunks: usize,
    degree_bits: usize,
    main_width: usize,
    logup_challenges: [EF4; 2],
) -> Proof<TabulaStarkConfig> {
    type P = TabulaPcs;
    type C = Challenger;

    let height = combined_trace.height();
    let num_quotient_chunks = 1 << log_num_quotient_chunks;
    let log_ext_degree = degree_bits;

    let pcs = config.pcs();
    let mut challenger = config.initialise_challenger();

    let trace_domain = <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, height);

    let (trace_commit, trace_data) =
        <P as Pcs<EF4, C>>::commit(pcs, [(trace_domain, combined_trace)]);

    let (preprocessed_commit, preprocessed_data_ref) = pp_prover
        .map(|pp| (pp.commitment, &pp.prover_data))
        .unzip();

    challenger.observe(BabyBear::from_u8(log_ext_degree as u8));
    challenger.observe(BabyBear::from_u8(degree_bits as u8));
    challenger.observe(BabyBear::from_usize(preprocessed_width));
    challenger.observe(trace_commit);
    if preprocessed_width > 0 {
        challenger.observe(*preprocessed_commit.as_ref().unwrap());
    }
    challenger.observe_slice(public_values);

    let alpha: EF4 = challenger.sample_algebra_element();

    let quotient_domain =
        trace_domain.create_disjoint_domain(1 << (log_ext_degree + log_num_quotient_chunks));

    let trace_on_quotient_domain =
        <P as Pcs<EF4, C>>::get_evaluations_on_domain(pcs, &trace_data, 0, quotient_domain);
    let preprocessed_on_quotient_domain = preprocessed_data_ref.map(|data| {
        <P as Pcs<EF4, C>>::get_evaluations_on_domain_no_random(pcs, data, 0, quotient_domain)
    });

    let quotient_values = quotient_values_rap(
        chip_ref,
        public_values,
        trace_domain,
        quotient_domain,
        &trace_on_quotient_domain,
        preprocessed_on_quotient_domain.as_ref(),
        alpha,
        inner_count,
        total_count,
        main_width,
        logup_challenges,
    );

    let quotient_flat = RowMajorMatrix::new_col(quotient_values).flatten_to_base();
    let (quotient_commit, quotient_data) = <P as Pcs<EF4, C>>::commit_quotient(
        pcs,
        quotient_domain,
        quotient_flat,
        num_quotient_chunks,
    );
    challenger.observe(quotient_commit);

    let commitments = Commitments {
        trace: trace_commit,
        quotient_chunks: quotient_commit,
        random: None,
    };

    let zeta: EF4 = challenger.sample_algebra_element();
    let zeta_next = trace_domain
        .next_point(zeta)
        .expect("domain should support next_point");

    let (opened_values, opening_proof) = {
        let round_trace = (&trace_data, vec![vec![zeta, zeta_next]]);
        let round_quotient = (&quotient_data, vec![vec![zeta]; num_quotient_chunks]);
        let round_preprocessed =
            preprocessed_data_ref.map(|data| (data, vec![vec![zeta, zeta_next]]));

        let rounds: Vec<_> = [round_trace, round_quotient]
            .into_iter()
            .chain(round_preprocessed)
            .collect();

        <P as Pcs<EF4, C>>::open_with_preprocessing(
            pcs,
            rounds,
            &mut challenger,
            preprocessed_data_ref.is_some(),
        )
    };

    let trace_local = opened_values[0][0][0].clone();
    let trace_next = opened_values[0][0][1].clone();
    let quotient_chunks: Vec<Vec<EF4>> = opened_values[1].iter().map(|v| v[0].clone()).collect();
    let (pp_local, pp_next) = if preprocessed_width > 0 {
        (
            Some(opened_values[2][0][0].clone()),
            Some(opened_values[2][0][1].clone()),
        )
    } else {
        (None, None)
    };

    Proof {
        commitments,
        opened_values: OpenedValues {
            trace_local,
            trace_next,
            preprocessed_local: pp_local,
            preprocessed_next: pp_next,
            quotient_chunks,
            random: None,
        },
        opening_proof,
        degree_bits: log_ext_degree,
    }
}

/// Two-phase quotient evaluation for a chip with interactions.
#[allow(clippy::too_many_arguments)]
fn quotient_values_rap<Mat, D>(
    air: &ChipRef<'_>,
    public_values: &[BabyBear],
    trace_domain: D,
    quotient_domain: D,
    trace_on_quotient_domain: &Mat,
    preprocessed_on_quotient_domain: Option<&Mat>,
    alpha: EF4,
    inner_count: usize,
    total_count: usize,
    main_width: usize,
    logup_challenges: [EF4; 2],
) -> Vec<EF4>
where
    Mat: Matrix<BabyBear> + Sync,
    D: PolynomialSpace<Val = BabyBear>,
{
    let quotient_size = quotient_domain.size();
    let combined_width = trace_on_quotient_domain.width();

    let qdb =
        quotient_size.trailing_zeros() as usize - trace_domain.size().trailing_zeros() as usize;
    let next_step = 1 << qdb;

    let mut sels = trace_domain.selectors_on_coset(quotient_domain);

    type PV = PackedVal<TabulaStarkConfig>;
    for _ in quotient_size..PV::WIDTH {
        sels.is_first_row.push(BabyBear::default());
        sels.is_last_row.push(BabyBear::default());
        sels.is_transition.push(BabyBear::default());
        sels.inv_vanishing.push(BabyBear::default());
    }

    let mut alpha_powers = Vec::with_capacity(total_count);
    let mut power = EF4::ONE;
    for _ in 0..total_count {
        alpha_powers.push(power);
        power *= alpha;
    }
    alpha_powers.reverse();

    let decomposed_alpha_powers: Vec<Vec<BabyBear>> = (0
        ..<EF4 as BasedVectorSpace<BabyBear>>::DIMENSION)
        .map(|i| {
            alpha_powers
                .iter()
                .map(|x| <EF4 as BasedVectorSpace<BabyBear>>::as_basis_coefficients_slice(x)[i])
                .collect()
        })
        .collect();

    type PC = PackedChallenge<TabulaStarkConfig>;

    let mut result = Vec::with_capacity(quotient_size);

    let mut i_start = 0;
    while i_start < quotient_size {
        let i_range = i_start..i_start + PV::WIDTH;

        let is_first_row = *PV::from_slice(&sels.is_first_row[i_range.clone()]);
        let is_last_row = *PV::from_slice(&sels.is_last_row[i_range.clone()]);
        let is_transition = *PV::from_slice(&sels.is_transition[i_range.clone()]);
        let inv_vanishing = *PV::from_slice(&sels.inv_vanishing[i_range]);

        let full_data = trace_on_quotient_domain.vertically_packed_row_pair(i_start, next_step);

        let mut truncated_data = Vec::with_capacity(2 * main_width);
        truncated_data.extend_from_slice(&full_data[..main_width]);
        truncated_data.extend_from_slice(&full_data[combined_width..combined_width + main_width]);
        let truncated_main = RowMajorMatrix::new(truncated_data, main_width);

        let full_main = RowMajorMatrix::new(full_data, combined_width);

        let preprocessed = preprocessed_on_quotient_domain.map(|pp| {
            let pp_width = pp.width();
            RowMajorMatrix::new(pp.vertically_packed_row_pair(i_start, next_step), pp_width)
        });

        // Phase 1: Inner chip constraints (truncated view).
        let mut folder1 = ProverConstraintFolder {
            main: truncated_main.as_view(),
            preprocessed: preprocessed.as_ref().map(|m| m.as_view()),
            public_values,
            is_first_row,
            is_last_row,
            is_transition,
            alpha_powers: &alpha_powers,
            decomposed_alpha_powers: &decomposed_alpha_powers,
            accumulator: PC::ZERO,
            constraint_index: 0,
        };
        air.eval(&mut folder1);

        debug_assert_eq!(
            folder1.constraint_index, inner_count,
            "inner chip produced unexpected constraint count"
        );

        // Phase 2: RAP constraints (full view via RapProverFolder).
        let mut rap_folder = RapProverFolder::new(
            truncated_main.as_view(),
            full_main.as_view(),
            preprocessed.as_ref().map(|m| m.as_view()),
            public_values,
            is_first_row,
            is_last_row,
            is_transition,
            &alpha_powers,
            folder1.accumulator,
            folder1.constraint_index,
            logup_challenges,
            main_width,
        );
        air.eval(&mut rap_folder);
        rap_folder.finalize_cumsum();

        debug_assert_eq!(
            rap_folder.constraint_index(), total_count,
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
