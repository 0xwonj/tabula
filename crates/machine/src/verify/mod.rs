//! STARK verification helpers shared across the proof pipeline.
//!
//! Provides per-sub-proof verification: chip manifest validation, PCS
//! verification, and per-chip constraint evaluation. The top-level
//! verify method lives in [`TabulaMachine::verify()`].

use tabula_stark::rap::verifier::RapVerifierFolder;

use std::collections::BTreeSet;

use p3_air::Air;
use p3_challenger::{CanObserve, FieldChallenger};
use p3_commit::{Pcs, PolynomialSpace};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrixView;
use p3_matrix::stack::VerticalPair;
use p3_uni_stark::{StarkGenericConfig, VerifierConstraintFolder};

use crate::chip_ref::ChipRef;
use crate::config::{
    Challenger, EF4, PcsCommitment, PcsDomain, PcsOpeningProof, TabulaPcs, TabulaStarkConfig,
};
use crate::keys::TabulaVerifyingKey;
use crate::proof::{ChipOpening, VerificationError};
use crate::registry::ChipRegistry;

/// One entry in the PCS verification batch: commitment + per-matrix opening points.
type PcsRound = (PcsCommitment, Vec<(PcsDomain, Vec<(EF4, Vec<EF4>)>)>);

// ─── Public API ─────────────────────────────────────────────────────────────

/// Verify a sub-proof given pre-computed LogUp challenges.
///
/// Used by [`TabulaMachine::verify()`] where challenges are derived from the
/// global (cross-proof) transcript rather than per-proof.
///
/// Does NOT check cross-proof bus balance (caller's responsibility).
#[allow(clippy::too_many_arguments)]
pub(crate) fn verify_sub_proof_with_challenges(
    config: &TabulaStarkConfig,
    registry: &ChipRegistry,
    vk: &TabulaVerifyingKey,
    chip_openings: &[ChipOpening],
    preprocessed_commitment: Option<PcsCommitment>,
    main_commitment: PcsCommitment,
    perm_commitment: Option<PcsCommitment>,
    quotient_commitment: PcsCommitment,
    opening_proof: &PcsOpeningProof,
    logup_challenges: [EF4; 2],
    challenger: &mut Challenger,
) -> Result<(), VerificationError> {
    let pcs = config.pcs();

    // ── Phase 0: Validate chip manifest ─────────────────────────────────
    validate_chip_manifest(vk, chip_openings)?;

    // ── Phase 1: Reconstruct per-proof challenges (alpha, zeta) ──────────
    if let Some(perm_c) = perm_commitment {
        challenger.observe(perm_c);
    }
    let alpha: EF4 = challenger.sample_algebra_element();
    challenger.observe(quotient_commitment);
    let zeta: EF4 = challenger.sample_algebra_element();

    // ── Phase 2-3: PCS verification ─────────────────────────────────────
    verify_pcs(
        pcs,
        chip_openings,
        main_commitment,
        perm_commitment,
        preprocessed_commitment,
        quotient_commitment,
        opening_proof,
        zeta,
        challenger,
    )?;

    // ── Phase 4: Per-chip constraint verification ───────────────────────
    verify_all_chip_constraints(pcs, registry, chip_openings, zeta, alpha, logup_challenges)?;

    Ok(())
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Build PCS verification rounds and run FRI verification.
#[allow(clippy::too_many_arguments)]
fn verify_pcs(
    pcs: &TabulaPcs,
    chip_openings: &[ChipOpening],
    main_commitment: PcsCommitment,
    perm_commitment: Option<PcsCommitment>,
    preprocessed_commitment: Option<PcsCommitment>,
    quotient_commitment: PcsCommitment,
    opening_proof: &PcsOpeningProof,
    zeta: EF4,
    challenger: &mut Challenger,
) -> Result<(), VerificationError> {
    let coms_to_verify = build_verification_rounds(
        pcs,
        chip_openings,
        main_commitment,
        perm_commitment,
        preprocessed_commitment,
        quotient_commitment,
        zeta,
    );
    <TabulaPcs as Pcs<EF4, Challenger>>::verify(pcs, coms_to_verify, opening_proof, challenger)
        .map_err(|e| VerificationError::PcsVerificationFailed {
            detail: format!("{e:?}"),
        })
}

/// Build PCS verification rounds (commitments + opening points) for Rounds 0-3.
#[allow(clippy::too_many_arguments)]
fn build_verification_rounds(
    pcs: &TabulaPcs,
    chip_openings: &[ChipOpening],
    main_commitment: PcsCommitment,
    perm_commitment: Option<PcsCommitment>,
    preprocessed_commitment: Option<PcsCommitment>,
    quotient_commitment: PcsCommitment,
    zeta: EF4,
) -> Vec<PcsRound> {
    type P = TabulaPcs;
    type C = Challenger;

    let rap_indices: Vec<usize> = chip_openings
        .iter()
        .enumerate()
        .filter(|(_, o)| o.perm_width > 0)
        .map(|(i, _)| i)
        .collect();
    let pp_indices: Vec<usize> = chip_openings
        .iter()
        .enumerate()
        .filter(|(_, o)| o.preprocessed_local.is_some())
        .map(|(i, _)| i)
        .collect();

    // Quotient chunk domains per chip.
    let q_domains: Vec<Vec<PcsDomain>> = chip_openings
        .iter()
        .map(|o| {
            let td = <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << o.degree_bits);
            let n = 1 << o.log_quotient_chunks;
            let qd = td.create_disjoint_domain(1 << (o.degree_bits + o.log_quotient_chunks));
            qd.split_domains(n)
                .iter()
                .map(|d| <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, d.size()))
                .collect()
        })
        .collect();

    let mut rounds = Vec::with_capacity(4);

    // Round 0: main traces.
    let main_matrices: Vec<_> = chip_openings
        .iter()
        .map(|o| {
            let dom = <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << o.degree_bits);
            let zn = dom
                .next_point(zeta)
                .expect("domain must support next_point for OOD evaluation");
            (
                dom,
                vec![(zeta, o.main_local.clone()), (zn, o.main_next.clone())],
            )
        })
        .collect();
    rounds.push((main_commitment, main_matrices));

    // Round 1: quotient chunks.
    let mut q_matrices = Vec::new();
    for (i, opening) in chip_openings.iter().enumerate() {
        for (qi, q_vals) in opening.quotient_chunks.iter().enumerate() {
            q_matrices.push((q_domains[i][qi], vec![(zeta, q_vals.clone())]));
        }
    }
    rounds.push((quotient_commitment, q_matrices));

    // Round 2: perm traces.
    if let Some(perm_c) = perm_commitment {
        let perm_matrices: Vec<_> = rap_indices
            .iter()
            .map(|&i| {
                let o = &chip_openings[i];
                let dom = <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << o.degree_bits);
                let zn = dom
                    .next_point(zeta)
                    .expect("domain must support next_point for perm trace");
                (
                    dom,
                    vec![(zeta, o.perm_local.clone()), (zn, o.perm_next.clone())],
                )
            })
            .collect();
        rounds.push((perm_c, perm_matrices));
    }

    // Round 3: preprocessed.
    if let Some(pp_c) = preprocessed_commitment {
        let pp_matrices: Vec<_> = pp_indices
            .iter()
            .map(|&i| {
                let o = &chip_openings[i];
                let dom = <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << o.degree_bits);
                let zn = dom
                    .next_point(zeta)
                    .expect("domain must support next_point for preprocessed trace");
                let pp_loc = o
                    .preprocessed_local
                    .clone()
                    .expect("preprocessed_local must exist for pp chip");
                let pp_nxt = o
                    .preprocessed_next
                    .clone()
                    .expect("preprocessed_next must exist for pp chip");
                (dom, vec![(zeta, pp_loc), (zn, pp_nxt)])
            })
            .collect();
        rounds.push((pp_c, pp_matrices));
    }

    rounds
}

/// Check that the chip openings contain exactly the expected set of chips.
pub(crate) fn validate_chip_manifest(
    vk: &TabulaVerifyingKey,
    chip_openings: &[ChipOpening],
) -> Result<(), VerificationError> {
    let proof_chips: BTreeSet<_> = chip_openings.iter().map(|o| o.chip_id).collect();
    let expected: BTreeSet<_> = vk.chip_ids().into_iter().collect();

    if proof_chips.len() != chip_openings.len() {
        return Err(VerificationError::InvalidChipManifest {
            detail: "duplicate chip IDs in proof".to_string(),
        });
    }
    let missing: Vec<_> = expected.difference(&proof_chips).copied().collect();
    let extra: Vec<_> = proof_chips.difference(&expected).copied().collect();
    if !missing.is_empty() || !extra.is_empty() {
        return Err(VerificationError::InvalidChipManifest {
            detail: format!("missing: {missing:?}, unexpected: {extra:?}"),
        });
    }
    Ok(())
}

/// Verify all chip constraints at the OOD point.
fn verify_all_chip_constraints(
    pcs: &TabulaPcs,
    registry: &ChipRegistry,
    chip_openings: &[ChipOpening],
    zeta: EF4,
    alpha: EF4,
    logup_challenges: [EF4; 2],
) -> Result<(), VerificationError> {
    for opening in chip_openings {
        let air = registry.get(opening.chip_id).ok_or_else(|| {
            VerificationError::InvalidChipManifest {
                detail: format!("unknown chip id: {}", opening.chip_id),
            }
        })?;
        let chip_ref = ChipRef::new(air);
        let trace_domain = <TabulaPcs as Pcs<EF4, Challenger>>::natural_domain_for_degree(
            pcs,
            1 << opening.degree_bits,
        );
        let num_q_chunks = 1 << opening.log_quotient_chunks;
        let q_domain = trace_domain
            .create_disjoint_domain(1 << (opening.degree_bits + opening.log_quotient_chunks));
        let sub_domains = q_domain.split_domains(num_q_chunks);
        let quotient = recompose_quotient(&sub_domains, &opening.quotient_chunks, zeta);

        verify_chip_constraints(
            &chip_ref,
            opening,
            trace_domain,
            zeta,
            alpha,
            quotient,
            logup_challenges,
        )
        .map_err(|detail| VerificationError::ChipVerificationFailed {
            chip_id: opening.chip_id,
            detail,
        })?;
    }
    Ok(())
}

/// Verify constraints at the OOD point for a single chip.
#[allow(clippy::too_many_arguments)]
fn verify_chip_constraints(
    chip_ref: &ChipRef<'_>,
    opening: &ChipOpening,
    trace_domain: PcsDomain,
    zeta: EF4,
    alpha: EF4,
    quotient: EF4,
    logup_challenges: [EF4; 2],
) -> Result<(), String> {
    let sels = trace_domain.selectors_at_point(zeta);

    let preprocessed = match (&opening.preprocessed_local, &opening.preprocessed_next) {
        (Some(local), Some(next)) => Some(VerticalPair::new(
            RowMajorMatrixView::new_row(local),
            RowMajorMatrixView::new_row(next),
        )),
        _ => None,
    };

    let ood_mismatch = "OOD evaluation mismatch: constraints(zeta) / Z_H(zeta) != quotient(zeta)";

    if opening.perm_width == 0 {
        let main = VerticalPair::new(
            RowMajorMatrixView::new_row(&opening.main_local),
            RowMajorMatrixView::new_row(&opening.main_next),
        );
        let mut folder = VerifierConstraintFolder {
            main,
            preprocessed,
            public_values: &opening.public_values,
            is_first_row: sels.is_first_row,
            is_last_row: sels.is_last_row,
            is_transition: sels.is_transition,
            alpha,
            accumulator: EF4::ZERO,
        };
        chip_ref.eval(&mut folder);
        if folder.accumulator * sels.inv_vanishing != quotient {
            return Err(ood_mismatch.to_string());
        }
    } else {
        let truncated_main = VerticalPair::new(
            RowMajorMatrixView::new_row(&opening.main_local),
            RowMajorMatrixView::new_row(&opening.main_next),
        );
        let mut full_local = opening.main_local.clone();
        full_local.extend_from_slice(&opening.perm_local);
        let mut full_next = opening.main_next.clone();
        full_next.extend_from_slice(&opening.perm_next);
        let full_main = VerticalPair::new(
            RowMajorMatrixView::new_row(&full_local),
            RowMajorMatrixView::new_row(&full_next),
        );

        let mut folder1 = VerifierConstraintFolder {
            main: truncated_main,
            preprocessed,
            public_values: &opening.public_values,
            is_first_row: sels.is_first_row,
            is_last_row: sels.is_last_row,
            is_transition: sels.is_transition,
            alpha,
            accumulator: EF4::ZERO,
        };
        chip_ref.eval(&mut folder1);

        let mut rap_folder = RapVerifierFolder::new(
            truncated_main,
            full_main,
            preprocessed,
            &opening.public_values,
            sels.is_first_row,
            sels.is_last_row,
            sels.is_transition,
            alpha,
            folder1.accumulator,
            logup_challenges,
            opening.main_width,
        );
        chip_ref.eval(&mut rap_folder);

        let coeffs = tabula_stark::rap::ef4::ef4_coeffs(opening.cumsum_final);
        rap_folder.finalize_cumsum(coeffs.map(EF4::from));

        if rap_folder.accumulator() * sels.inv_vanishing != quotient {
            return Err(ood_mismatch.to_string());
        }
    }
    Ok(())
}

/// Recompose quotient polynomial from its chunk evaluations.
fn recompose_quotient(
    quotient_chunk_domains: &[PcsDomain],
    quotient_chunk_values: &[Vec<EF4>],
    zeta: EF4,
) -> EF4 {
    p3_uni_stark::recompose_quotient_from_chunks::<TabulaStarkConfig>(
        quotient_chunk_domains,
        quotient_chunk_values,
        zeta,
    )
}
