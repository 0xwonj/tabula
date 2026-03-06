//! Per-chip verification pipeline: standard and RAP (two-phase) variants.

use p3_air::Air;
use p3_baby_bear::BabyBear;
use p3_challenger::{CanObserve, FieldChallenger};
use p3_commit::{Pcs, PolynomialSpace};
use p3_field::{BasedVectorSpace, PrimeCharacteristicRing};
use p3_matrix::dense::RowMajorMatrixView;
use p3_matrix::stack::VerticalPair;
use p3_uni_stark::{
    StarkGenericConfig, VerifierConstraintFolder, get_log_num_quotient_chunks,
    recompose_quotient_from_chunks,
};

use crate::chip_ref::ChipRef;
use crate::config::{Challenger, EF4, TabulaPcs, TabulaStarkConfig};
use crate::proof::{ChipProofEntry, VerificationError};

use super::RapVerifierFolder;

/// Concrete PCS domain type for return values.
type PcsDomain = <TabulaPcs as p3_commit::Pcs<EF4, Challenger>>::Domain;

// ─── Per-chip verifiers ─────────────────────────────────────────────────────

pub(super) fn verify_chip_standard(
    config: &TabulaStarkConfig,
    chip_ref: &ChipRef<'_>,
    entry: &ChipProofEntry,
) -> Result<(), VerificationError> {
    p3_uni_stark::verify_with_preprocessed(
        config,
        chip_ref,
        &entry.proof,
        &entry.public_values,
        entry.preprocessed_vk.as_ref(),
    )
    .map_err(|e| VerificationError::ChipVerificationFailed {
        chip_id: entry.chip_id,
        detail: format!("{e:?}"),
    })
}

pub(super) fn verify_chip_rap(
    config: &TabulaStarkConfig,
    chip_ref: &ChipRef<'_>,
    entry: &ChipProofEntry,
    logup_challenges: [EF4; 2],
) -> Result<(), VerificationError> {
    let combined_width = entry.main_width + entry.perm_width;
    let degree_bits = entry.proof.degree_bits;
    let preprocessed_width = entry.preprocessed_vk.as_ref().map_or(0, |vk| vk.width);

    let inner_log =
        get_log_num_quotient_chunks(chip_ref, preprocessed_width, entry.public_values.len(), 0);
    let log_num_quotient_chunks = inner_log.max(2);
    let num_quotient_chunks = 1 << log_num_quotient_chunks;

    let opened = &entry.proof.opened_values;
    if opened.trace_local.len() != combined_width
        || opened.trace_next.len() != combined_width
        || opened.quotient_chunks.len() != num_quotient_chunks
        || opened
            .quotient_chunks
            .iter()
            .any(|qc| qc.len() != <EF4 as BasedVectorSpace<BabyBear>>::DIMENSION)
    {
        return Err(VerificationError::ChipVerificationFailed {
            chip_id: entry.chip_id,
            detail: "invalid proof shape".to_string(),
        });
    }

    if let Some(vk) = &entry.preprocessed_vk
        && (vk.width != preprocessed_width || vk.degree_bits != degree_bits)
    {
        return Err(VerificationError::ChipVerificationFailed {
            chip_id: entry.chip_id,
            detail: "preprocessed data inconsistency".to_string(),
        });
    }

    let (trace_domain, zeta, alpha, quotient) =
        pcs_verify_and_recompose(config, entry, preprocessed_width, log_num_quotient_chunks)?;

    verify_constraints_rap(
        chip_ref,
        &opened.trace_local,
        &opened.trace_next,
        opened.preprocessed_local.as_deref(),
        opened.preprocessed_next.as_deref(),
        &entry.public_values,
        trace_domain,
        zeta,
        alpha,
        quotient,
        entry.main_width,
        logup_challenges,
    )
    .map_err(|detail| VerificationError::ChipVerificationFailed {
        chip_id: entry.chip_id,
        detail,
    })
}

// ─── PCS verification ────────────────────────────────────────────────────────

/// Verify PCS opening proof and recompose the quotient polynomial.
///
/// Returns `(trace_domain, zeta, alpha, quotient)` on success.
fn pcs_verify_and_recompose(
    config: &TabulaStarkConfig,
    entry: &ChipProofEntry,
    preprocessed_width: usize,
    log_num_quotient_chunks: usize,
) -> Result<(PcsDomain, EF4, EF4, EF4), VerificationError> {
    type P = TabulaPcs;
    type C = Challenger;

    let degree_bits = entry.proof.degree_bits;
    let degree = 1 << degree_bits;
    let num_quotient_chunks = 1 << log_num_quotient_chunks;
    let opened = &entry.proof.opened_values;

    let pcs = config.pcs();
    let trace_domain = <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, degree);
    let quotient_domain =
        trace_domain.create_disjoint_domain(1 << (degree_bits + log_num_quotient_chunks));
    let quotient_chunks_domains = quotient_domain.split_domains(num_quotient_chunks);

    let randomized_quotient_chunks_domains: Vec<_> = quotient_chunks_domains
        .iter()
        .map(|domain| <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, domain.size()))
        .collect();

    let mut challenger = config.initialise_challenger();
    challenger.observe(BabyBear::from_u8(degree_bits as u8));
    challenger.observe(BabyBear::from_u8(degree_bits as u8));
    challenger.observe(BabyBear::from_usize(preprocessed_width));
    challenger.observe(entry.proof.commitments.trace);
    if preprocessed_width > 0 {
        let vk = entry.preprocessed_vk.as_ref().unwrap();
        challenger.observe(vk.commitment);
    }
    challenger.observe_slice(&entry.public_values);

    let alpha: EF4 = challenger.sample_algebra_element();
    challenger.observe(entry.proof.commitments.quotient_chunks);

    let zeta: EF4 = challenger.sample_algebra_element();
    let zeta_next =
        trace_domain
            .next_point(zeta)
            .ok_or_else(|| VerificationError::ChipVerificationFailed {
                chip_id: entry.chip_id,
                detail: "next point unavailable".to_string(),
            })?;

    let mut coms_to_verify = vec![
        (
            entry.proof.commitments.trace,
            vec![(
                trace_domain,
                vec![
                    (zeta, opened.trace_local.clone()),
                    (zeta_next, opened.trace_next.clone()),
                ],
            )],
        ),
        (
            entry.proof.commitments.quotient_chunks,
            randomized_quotient_chunks_domains
                .iter()
                .zip(&opened.quotient_chunks)
                .map(|(domain, values)| (*domain, vec![(zeta, values.clone())]))
                .collect(),
        ),
    ];

    if preprocessed_width > 0 {
        let vk = entry.preprocessed_vk.as_ref().unwrap();
        coms_to_verify.push((
            vk.commitment,
            vec![(
                trace_domain,
                vec![
                    (zeta, opened.preprocessed_local.clone().unwrap()),
                    (zeta_next, opened.preprocessed_next.clone().unwrap()),
                ],
            )],
        ));
    }

    <P as Pcs<EF4, C>>::verify(
        pcs,
        coms_to_verify,
        &entry.proof.opening_proof,
        &mut challenger,
    )
    .map_err(|e| VerificationError::ChipVerificationFailed {
        chip_id: entry.chip_id,
        detail: format!("PCS verification failed: {e:?}"),
    })?;

    let quotient = recompose_quotient_from_chunks::<TabulaStarkConfig>(
        &quotient_chunks_domains,
        &opened.quotient_chunks,
        zeta,
    );

    Ok((trace_domain, zeta, alpha, quotient))
}

// ─── Two-phase constraint verification ────────────────────────────────────

/// Verify constraints at the out-of-domain point using two-phase evaluation.
#[allow(clippy::too_many_arguments)]
fn verify_constraints_rap<A, D>(
    air: &A,
    trace_local: &[EF4],
    trace_next: &[EF4],
    preprocessed_local: Option<&[EF4]>,
    preprocessed_next: Option<&[EF4]>,
    public_values: &[BabyBear],
    trace_domain: D,
    zeta: EF4,
    alpha: EF4,
    quotient: EF4,
    main_width: usize,
    logup_challenges: [EF4; 2],
) -> Result<(), String>
where
    A: for<'a> Air<VerifierConstraintFolder<'a, TabulaStarkConfig>>
        + for<'a> Air<RapVerifierFolder<'a>>,
    D: PolynomialSpace<Val = BabyBear>,
{
    let sels = trace_domain.selectors_at_point(zeta);

    let truncated_local = &trace_local[..main_width];
    let truncated_next = &trace_next[..main_width];

    let truncated_main = VerticalPair::new(
        RowMajorMatrixView::new_row(truncated_local),
        RowMajorMatrixView::new_row(truncated_next),
    );

    let preprocessed = match (preprocessed_local, preprocessed_next) {
        (Some(local), Some(next)) => Some(VerticalPair::new(
            RowMajorMatrixView::new_row(local),
            RowMajorMatrixView::new_row(next),
        )),
        _ => None,
    };

    let mut folder1 = VerifierConstraintFolder {
        main: truncated_main,
        preprocessed,
        public_values,
        is_first_row: sels.is_first_row,
        is_last_row: sels.is_last_row,
        is_transition: sels.is_transition,
        alpha,
        accumulator: EF4::ZERO,
    };
    air.eval(&mut folder1);

    let full_main = VerticalPair::new(
        RowMajorMatrixView::new_row(trace_local),
        RowMajorMatrixView::new_row(trace_next),
    );

    let mut rap_folder = RapVerifierFolder::new(
        truncated_main,
        full_main,
        preprocessed,
        public_values,
        sels.is_first_row,
        sels.is_last_row,
        sels.is_transition,
        alpha,
        folder1.accumulator,
        logup_challenges,
        main_width,
    );
    air.eval(&mut rap_folder);
    rap_folder.finalize_cumsum();

    let folded_constraints = rap_folder.accumulator();
    if folded_constraints * sels.inv_vanishing != quotient {
        return Err(
            "OOD evaluation mismatch: constraints(zeta) / Z_H(zeta) != quotient(zeta)".to_string(),
        );
    }

    Ok(())
}
