//! Batched multi-chip STARK verifier with shared PCS.
//!
//! Mirrors the prover's 3-round Fiat-Shamir transcript to reconstruct
//! challenges, then verifies:
//! 1. Single PCS opening proof for all committed data
//! 2. Per-chip constraint evaluation at the OOD point
//! 3. Cross-chip LogUp balance (Σ cumsums = 0)

pub(crate) mod rap_folder;

pub(crate) use rap_folder::RapVerifierFolder;

use std::collections::BTreeSet;

use p3_air::Air;
use p3_challenger::{CanObserve, CanSample, FieldChallenger};
use p3_commit::{Pcs, PolynomialSpace};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrixView;
use p3_matrix::stack::VerticalPair;
use p3_uni_stark::{StarkGenericConfig, VerifierConstraintFolder};

use crate::chip_ref::ChipRef;
use crate::config::{Challenger, EF4, PcsCommitment, PcsDomain, TabulaPcs, TabulaStarkConfig};
use crate::keys::TabulaVerifyingKey;
use crate::proof::{ChipOpening, TabulaProof, VerificationError};
use crate::registry::ChipRegistry;

/// One entry in the PCS verification batch: commitment + per-matrix opening points.
type PcsRound = (PcsCommitment, Vec<(PcsDomain, Vec<(EF4, Vec<EF4>)>)>);

// ─── Public API ─────────────────────────────────────────────────────────────

/// Verify a Tabula STARK proof with batched PCS.
pub fn verify_with_key(
    config: &TabulaStarkConfig,
    registry: &ChipRegistry,
    vk: &TabulaVerifyingKey,
    proof: &TabulaProof,
) -> Result<(), VerificationError> {
    let pcs = config.pcs();
    let mut challenger = config.initialise_challenger();

    // ── Phase 0: Validate chip manifest ─────────────────────────────────
    validate_chip_manifest(vk, proof)?;

    // ── Phase 1: Reconstruct Fiat-Shamir transcript ─────────────────────
    let (logup_challenges, alpha, zeta) = reconstruct_challenges(proof, &mut challenger);

    // ── Phase 2: Build PCS verification data ────────────────────────────
    let coms_to_verify = build_verification_rounds(pcs, proof, zeta);

    // ── Phase 3: PCS verification ───────────────────────────────────────
    <TabulaPcs as Pcs<EF4, Challenger>>::verify(
        pcs, coms_to_verify, &proof.opening_proof, &mut challenger,
    )
    .map_err(|e| VerificationError::PcsVerificationFailed {
        detail: format!("{e:?}"),
    })?;

    // ── Phase 4: Per-chip constraint verification ───────────────────────
    for opening in &proof.chip_openings {
        let air = registry
            .get(opening.chip_id)
            .ok_or_else(|| VerificationError::InvalidChipManifest {
                detail: format!("unknown chip id: {}", opening.chip_id),
            })?;
        let chip_ref = ChipRef::new(air);
        let trace_domain = <TabulaPcs as Pcs<EF4, Challenger>>::natural_domain_for_degree(
            pcs, 1 << opening.degree_bits,
        );
        let num_q_chunks = 1 << opening.log_quotient_chunks;
        let q_domain = trace_domain
            .create_disjoint_domain(1 << (opening.degree_bits + opening.log_quotient_chunks));
        let sub_domains = q_domain.split_domains(num_q_chunks);
        let quotient = recompose_quotient(&sub_domains, &opening.quotient_chunks, zeta);

        verify_chip_constraints(
            &chip_ref, opening, trace_domain, zeta, alpha, quotient, logup_challenges,
        )
        .map_err(|detail| VerificationError::ChipVerificationFailed {
            chip_id: opening.chip_id,
            detail,
        })?;
    }

    // ── Phases 5-6: LogUp balance + public value consistency ────────────
    verify_logup_and_public_values(vk, proof)
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Reconstruct Fiat-Shamir challenges from the proof transcript.
///
/// Returns `(logup_challenges, alpha, zeta)` matching the prover's 3-round
/// protocol.
fn reconstruct_challenges(
    proof: &TabulaProof,
    challenger: &mut Challenger,
) -> ([EF4; 2], EF4, EF4) {
    let statement_felts = proof.statement.to_field_elements();
    challenger.observe_slice(&statement_felts);
    if let Some(ref pp_c) = proof.preprocessed_commitment {
        challenger.observe(*pp_c);
    }
    challenger.observe(proof.main_commitment);

    let logup_alpha: EF4 = challenger.sample();
    let logup_beta: EF4 = challenger.sample();

    if let Some(ref perm_c) = proof.perm_commitment {
        challenger.observe(*perm_c);
    }
    let alpha: EF4 = challenger.sample_algebra_element();

    challenger.observe(proof.quotient_commitment);
    let zeta: EF4 = challenger.sample_algebra_element();

    ([logup_alpha, logup_beta], alpha, zeta)
}

/// Build PCS verification rounds (commitments + opening points) for Rounds 0-3.
fn build_verification_rounds(
    pcs: &TabulaPcs,
    proof: &TabulaProof,
    zeta: EF4,
) -> Vec<PcsRound> {
    type P = TabulaPcs;
    type C = Challenger;

    let rap_indices: Vec<usize> = proof.chip_openings.iter().enumerate()
        .filter(|(_, o)| o.perm_width > 0).map(|(i, _)| i).collect();
    let pp_indices: Vec<usize> = proof.chip_openings.iter().enumerate()
        .filter(|(_, o)| o.preprocessed_local.is_some()).map(|(i, _)| i).collect();

    // Quotient chunk domains per chip
    let q_domains: Vec<Vec<PcsDomain>> = proof.chip_openings.iter().map(|o| {
        let td = <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << o.degree_bits);
        let n = 1 << o.log_quotient_chunks;
        let qd = td.create_disjoint_domain(1 << (o.degree_bits + o.log_quotient_chunks));
        qd.split_domains(n).iter()
            .map(|d| <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, d.size()))
            .collect()
    }).collect();

    let mut rounds = Vec::with_capacity(4);

    // Round 0: main traces
    let main_matrices: Vec<_> = proof.chip_openings.iter().map(|o| {
        let dom = <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << o.degree_bits);
        let zn = dom.next_point(zeta)
            .expect("domain must support next_point for OOD evaluation");
        (dom, vec![(zeta, o.main_local.clone()), (zn, o.main_next.clone())])
    }).collect();
    rounds.push((proof.main_commitment, main_matrices));

    // Round 1: quotient chunks
    let mut q_matrices = Vec::new();
    for (i, opening) in proof.chip_openings.iter().enumerate() {
        for (qi, q_vals) in opening.quotient_chunks.iter().enumerate() {
            q_matrices.push((q_domains[i][qi], vec![(zeta, q_vals.clone())]));
        }
    }
    rounds.push((proof.quotient_commitment, q_matrices));

    // Round 2: perm traces
    if let Some(perm_c) = proof.perm_commitment {
        let perm_matrices: Vec<_> = rap_indices.iter().map(|&i| {
            let o = &proof.chip_openings[i];
            let dom = <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << o.degree_bits);
            let zn = dom.next_point(zeta)
                .expect("domain must support next_point for perm trace");
            (dom, vec![(zeta, o.perm_local.clone()), (zn, o.perm_next.clone())])
        }).collect();
        rounds.push((perm_c, perm_matrices));
    }

    // Round 3: preprocessed
    if let Some(pp_c) = proof.preprocessed_commitment {
        let pp_matrices: Vec<_> = pp_indices.iter().map(|&i| {
            let o = &proof.chip_openings[i];
            let dom = <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << o.degree_bits);
            let zn = dom.next_point(zeta)
                .expect("domain must support next_point for preprocessed trace");
            let pp_loc = o.preprocessed_local.clone()
                .expect("preprocessed_local must exist for pp chip");
            let pp_nxt = o.preprocessed_next.clone()
                .expect("preprocessed_next must exist for pp chip");
            (dom, vec![(zeta, pp_loc), (zn, pp_nxt)])
        }).collect();
        rounds.push((pp_c, pp_matrices));
    }

    rounds
}

/// Verify cross-chip LogUp cumulative sum balance and public value consistency.
fn verify_logup_and_public_values(
    vk: &TabulaVerifyingKey,
    proof: &TabulaProof,
) -> Result<(), VerificationError> {
    let cumsum_total: EF4 = proof.chip_openings.iter()
        .map(|o| o.cumsum_final)
        .fold(EF4::ZERO, |acc, c| acc + c);
    if cumsum_total != EF4::ZERO {
        return Err(VerificationError::LogUpImbalance {
            total: crate::ef4::ef4_coeffs(cumsum_total),
        });
    }

    let expected_pvs = proof.statement.to_field_elements();
    for opening in &proof.chip_openings {
        let info = vk.get(opening.chip_id).ok_or_else(|| {
            VerificationError::InvalidChipManifest {
                detail: format!("no verify info for chip {}", opening.chip_id),
            }
        })?;
        if info.num_public_values > 0 && opening.public_values != expected_pvs {
            return Err(VerificationError::InvalidChipManifest {
                detail: format!(
                    "chip {} public values do not match proof statement",
                    opening.chip_id
                ),
            });
        }
    }
    Ok(())
}

/// Check that the proof contains exactly the expected set of chips.
fn validate_chip_manifest(
    vk: &TabulaVerifyingKey,
    proof: &TabulaProof,
) -> Result<(), VerificationError> {
    let proof_chips: BTreeSet<_> = proof.chip_openings.iter().map(|o| o.chip_id).collect();
    let expected: BTreeSet<_> = vk.chip_ids().into_iter().collect();

    if proof_chips.len() != proof.chip_openings.len() {
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

    let ood_mismatch =
        "OOD evaluation mismatch: constraints(zeta) / Z_H(zeta) != quotient(zeta)";

    if opening.perm_width == 0 {
        let main = VerticalPair::new(
            RowMajorMatrixView::new_row(&opening.main_local),
            RowMajorMatrixView::new_row(&opening.main_next),
        );
        let mut folder = VerifierConstraintFolder {
            main, preprocessed,
            public_values: &opening.public_values,
            is_first_row: sels.is_first_row,
            is_last_row: sels.is_last_row,
            is_transition: sels.is_transition,
            alpha, accumulator: EF4::ZERO,
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
            main: truncated_main, preprocessed,
            public_values: &opening.public_values,
            is_first_row: sels.is_first_row,
            is_last_row: sels.is_last_row,
            is_transition: sels.is_transition,
            alpha, accumulator: EF4::ZERO,
        };
        chip_ref.eval(&mut folder1);

        let mut rap_folder = RapVerifierFolder::new(
            truncated_main, full_main, preprocessed,
            &opening.public_values,
            sels.is_first_row, sels.is_last_row, sels.is_transition,
            alpha, folder1.accumulator, logup_challenges, opening.main_width,
        );
        chip_ref.eval(&mut rap_folder);

        let coeffs = crate::ef4::ef4_coeffs(opening.cumsum_final);
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
        quotient_chunk_domains, quotient_chunk_values, zeta,
    )
}
