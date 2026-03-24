//! STARK verification helpers shared across the proof pipeline.

use std::collections::BTreeSet;

use p3_air::{Air, RowWindow};
use p3_challenger::{CanObserve, FieldChallenger};
use p3_commit::{Pcs, PolynomialSpace};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::dense::RowMajorMatrixView;
use p3_matrix::stack::VerticalPair;
use p3_uni_stark::{StarkGenericConfig, VerifierConstraintFolder};
use tabula_stark::rap::verifier::RapVerifierFolder;

use crate::config::{
    Challenger, EF4, PcsCommitment, PcsDomain, PcsOpeningProof, TabulaPcs, TabulaStarkConfig,
};
use crate::proof::chip_ref::ChipRef;
use crate::proof::errors::VerificationError;
use crate::proof::model::ChipOpening;
use crate::proof::opening_shape::{preprocessed_opening_points, transition_opening_points};
use crate::setup::metadata::{ChipVerificationMetadata, TierVerificationMetadata};
use crate::setup::registry::ChipRegistry;

type PcsRound = (PcsCommitment, Vec<(PcsDomain, Vec<(EF4, Vec<EF4>)>)>);

#[allow(clippy::too_many_arguments)]
pub(crate) fn verify_sub_proof_with_challenges(
    config: &TabulaStarkConfig,
    registry: &ChipRegistry,
    verification_metadata: &TierVerificationMetadata,
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

    validate_chip_manifest(verification_metadata, chip_openings)?;

    if let Some(ref perm_commitment) = perm_commitment {
        challenger.observe(perm_commitment.clone());
    }
    let alpha: EF4 = challenger.sample_algebra_element();
    challenger.observe(quotient_commitment.clone());
    let zeta: EF4 = challenger.sample_algebra_element();

    verify_pcs(
        pcs,
        verification_metadata,
        chip_openings,
        main_commitment,
        perm_commitment,
        preprocessed_commitment,
        quotient_commitment,
        opening_proof,
        zeta,
        challenger,
    )?;

    verify_all_chip_constraints(pcs, registry, chip_openings, zeta, alpha, logup_challenges)?;

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn verify_pcs(
    pcs: &TabulaPcs,
    verification_metadata: &TierVerificationMetadata,
    chip_openings: &[ChipOpening],
    main_commitment: PcsCommitment,
    perm_commitment: Option<PcsCommitment>,
    preprocessed_commitment: Option<PcsCommitment>,
    quotient_commitment: PcsCommitment,
    opening_proof: &PcsOpeningProof,
    zeta: EF4,
    challenger: &mut Challenger,
) -> Result<(), VerificationError> {
    let rounds = build_verification_rounds(
        pcs,
        verification_metadata,
        chip_openings,
        main_commitment,
        perm_commitment,
        preprocessed_commitment,
        quotient_commitment,
        zeta,
    );
    <TabulaPcs as Pcs<EF4, Challenger>>::verify(pcs, rounds, opening_proof, challenger).map_err(
        |error| VerificationError::PcsVerificationFailed {
            detail: format!("{error:?}"),
        },
    )
}

#[allow(clippy::too_many_arguments)]
fn build_verification_rounds(
    pcs: &TabulaPcs,
    verification_metadata: &TierVerificationMetadata,
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
        .filter(|(_, opening)| opening.perm_width > 0)
        .map(|(index, _)| index)
        .collect();
    let pp_indices: Vec<usize> = chip_openings
        .iter()
        .enumerate()
        .filter(|(_, opening)| opening.preprocessed_local.is_some())
        .map(|(index, _)| index)
        .collect();

    let quotient_domains: Vec<Vec<PcsDomain>> = chip_openings
        .iter()
        .map(|opening| {
            let trace_domain =
                <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << opening.degree_bits);
            let num_chunks = 1 << opening.log_quotient_chunks;
            let quotient_domain = trace_domain
                .create_disjoint_domain(1 << (opening.degree_bits + opening.log_quotient_chunks));
            quotient_domain
                .split_domains(num_chunks)
                .iter()
                .map(|domain: &PcsDomain| {
                    <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, domain.size())
                })
                .collect()
        })
        .collect();

    let mut rounds = Vec::with_capacity(4);

    let main_matrices: Vec<_> = chip_openings
        .iter()
        .map(|opening| {
            let domain =
                <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << opening.degree_bits);
            let [zeta, zeta_next] = transition_opening_points(pcs, opening.degree_bits, zeta);
            (
                domain,
                vec![
                    (zeta, opening.main_local.clone()),
                    (zeta_next, opening.main_next.clone()),
                ],
            )
        })
        .collect();
    rounds.push((main_commitment, main_matrices));

    let mut quotient_matrices = Vec::new();
    for (index, opening) in chip_openings.iter().enumerate() {
        for (chunk_index, chunk_values) in opening.quotient_chunks.iter().enumerate() {
            quotient_matrices.push((
                quotient_domains[index][chunk_index],
                vec![(zeta, chunk_values.clone())],
            ));
        }
    }
    rounds.push((quotient_commitment, quotient_matrices));

    if let Some(perm_commitment) = perm_commitment {
        let perm_matrices: Vec<_> = rap_indices
            .iter()
            .map(|&index| {
                let opening = &chip_openings[index];
                let domain =
                    <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << opening.degree_bits);
                let [zeta, zeta_next] = transition_opening_points(pcs, opening.degree_bits, zeta);
                (
                    domain,
                    vec![
                        (zeta, opening.perm_local.clone()),
                        (zeta_next, opening.perm_next.clone()),
                    ],
                )
            })
            .collect();
        rounds.push((perm_commitment, perm_matrices));
    }

    if let Some(preprocessed_commitment) = preprocessed_commitment {
        let preprocessed_matrices: Vec<_> = pp_indices
            .iter()
            .map(|&index| {
                let opening = &chip_openings[index];
                let metadata = verification_metadata
                    .get(opening.chip_id)
                    .expect("validated chip metadata");
                let domain =
                    <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << opening.degree_bits);
                let points = preprocessed_opening_points(
                    pcs,
                    opening.degree_bits,
                    zeta,
                    !metadata.preprocessed_next_row_columns.is_empty(),
                );
                let mut point_values = vec![(
                    points[0],
                    opening
                        .preprocessed_local
                        .clone()
                        .expect("validated preprocessed opening"),
                )];
                if points.len() == 2 {
                    point_values.push((
                        points[1],
                        opening
                            .preprocessed_next
                            .clone()
                            .expect("validated preprocessed next opening"),
                    ));
                }
                (domain, point_values)
            })
            .collect();
        rounds.push((preprocessed_commitment, preprocessed_matrices));
    }

    rounds
}

pub(crate) fn validate_chip_manifest(
    verification_metadata: &TierVerificationMetadata,
    chip_openings: &[ChipOpening],
) -> Result<(), VerificationError> {
    let proof_chips: BTreeSet<_> = chip_openings
        .iter()
        .map(|opening| opening.chip_id)
        .collect();
    let expected: BTreeSet<_> = verification_metadata.chip_ids().into_iter().collect();

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

    for opening in chip_openings {
        let metadata = verification_metadata
            .get(opening.chip_id)
            .expect("chip presence already checked");
        validate_opening_shapes(opening, metadata)?;
    }

    Ok(())
}

fn validate_opening_shapes(
    opening: &ChipOpening,
    metadata: &ChipVerificationMetadata,
) -> Result<(), VerificationError> {
    if opening.main_width != metadata.main_width {
        return Err(VerificationError::InvalidChipManifest {
            detail: format!(
                "chip '{}' main width {} does not match metadata width {}",
                opening.chip_id, opening.main_width, metadata.main_width
            ),
        });
    }
    if opening.main_local.len() != metadata.main_width
        || opening.main_next.len() != metadata.main_width
    {
        return Err(VerificationError::InvalidChipManifest {
            detail: format!(
                "chip '{}' main opening lengths do not match metadata width {}",
                opening.chip_id, metadata.main_width
            ),
        });
    }
    if opening.public_values.len() != metadata.num_public_values {
        return Err(VerificationError::InvalidChipManifest {
            detail: format!(
                "chip '{}' public values length {} does not match metadata length {}",
                opening.chip_id,
                opening.public_values.len(),
                metadata.num_public_values
            ),
        });
    }

    let expected_perm_width = if metadata.interactions_per_row == 0 {
        0
    } else {
        4 * (metadata.interactions_per_row + 1)
    };
    if opening.perm_width != expected_perm_width
        || opening.perm_local.len() != expected_perm_width
        || opening.perm_next.len() != expected_perm_width
    {
        return Err(VerificationError::InvalidChipManifest {
            detail: format!(
                "chip '{}' permutation opening widths do not match metadata width {}",
                opening.chip_id, expected_perm_width
            ),
        });
    }

    if metadata.preprocessed_width == 0 {
        if opening.preprocessed_local.is_some() || opening.preprocessed_next.is_some() {
            return Err(VerificationError::InvalidChipManifest {
                detail: format!(
                    "chip '{}' provided unexpected preprocessed openings",
                    opening.chip_id
                ),
            });
        }
        return Ok(());
    }

    let local = opening.preprocessed_local.as_ref().ok_or_else(|| {
        VerificationError::InvalidChipManifest {
            detail: format!(
                "chip '{}' is missing required preprocessed local openings",
                opening.chip_id
            ),
        }
    })?;
    if local.len() != metadata.preprocessed_width {
        return Err(VerificationError::InvalidChipManifest {
            detail: format!(
                "chip '{}' preprocessed local width {} does not match metadata width {}",
                opening.chip_id,
                local.len(),
                metadata.preprocessed_width
            ),
        });
    }

    let expects_next = !metadata.preprocessed_next_row_columns.is_empty();
    match (expects_next, opening.preprocessed_next.as_ref()) {
        (true, Some(next)) if next.len() == metadata.preprocessed_width => Ok(()),
        (true, Some(next)) => Err(VerificationError::InvalidChipManifest {
            detail: format!(
                "chip '{}' preprocessed next width {} does not match metadata width {}",
                opening.chip_id,
                next.len(),
                metadata.preprocessed_width
            ),
        }),
        (true, None) => Err(VerificationError::InvalidChipManifest {
            detail: format!(
                "chip '{}' is missing required preprocessed next-row openings",
                opening.chip_id
            ),
        }),
        (false, Some(_)) => Err(VerificationError::InvalidChipManifest {
            detail: format!(
                "chip '{}' provided unexpected preprocessed next-row openings",
                opening.chip_id
            ),
        }),
        (false, None) => Ok(()),
    }
}

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
        let quotient_domain = trace_domain
            .create_disjoint_domain(1 << (opening.degree_bits + opening.log_quotient_chunks));
        let quotient_chunk_domains = quotient_domain.split_domains(num_q_chunks);
        let quotient = recompose_quotient(&quotient_chunk_domains, &opening.quotient_chunks, zeta);

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
    let selectors = trace_domain.selectors_at_point(zeta);

    let preprocessed = match (&opening.preprocessed_local, &opening.preprocessed_next) {
        (Some(local), Some(next)) => VerticalPair::new(
            RowMajorMatrixView::new_row(local),
            RowMajorMatrixView::new_row(next),
        ),
        (Some(local), None) => VerticalPair::new(
            RowMajorMatrixView::new_row(local),
            RowMajorMatrixView::new_row(local),
        ),
        _ => VerticalPair::new(
            RowMajorMatrixView::new(&[], 0),
            RowMajorMatrixView::new(&[], 0),
        ),
    };
    let preprocessed_window =
        RowWindow::from_two_rows(preprocessed.top.values, preprocessed.bottom.values);

    let ood_mismatch = "OOD evaluation mismatch: constraints(zeta) / Z_H(zeta) != quotient(zeta)";

    if opening.perm_width == 0 {
        let main = VerticalPair::new(
            RowMajorMatrixView::new_row(&opening.main_local),
            RowMajorMatrixView::new_row(&opening.main_next),
        );
        let mut folder = VerifierConstraintFolder {
            main,
            preprocessed,
            preprocessed_window,
            public_values: &opening.public_values,
            is_first_row: selectors.is_first_row,
            is_last_row: selectors.is_last_row,
            is_transition: selectors.is_transition,
            alpha,
            accumulator: EF4::ZERO,
        };
        chip_ref.eval(&mut folder);
        if folder.accumulator * selectors.inv_vanishing != quotient {
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

        let preprocessed_opt = match (&opening.preprocessed_local, &opening.preprocessed_next) {
            (Some(_), _) => Some(preprocessed),
            _ => None,
        };

        let mut folder = VerifierConstraintFolder {
            main: truncated_main,
            preprocessed,
            preprocessed_window,
            public_values: &opening.public_values,
            is_first_row: selectors.is_first_row,
            is_last_row: selectors.is_last_row,
            is_transition: selectors.is_transition,
            alpha,
            accumulator: EF4::ZERO,
        };
        chip_ref.eval(&mut folder);

        let mut rap_folder = RapVerifierFolder::new(
            truncated_main,
            full_main,
            preprocessed_opt,
            &opening.public_values,
            selectors.is_first_row,
            selectors.is_last_row,
            selectors.is_transition,
            alpha,
            folder.accumulator,
            logup_challenges,
            opening.main_width,
        );
        chip_ref.eval(&mut rap_folder);

        let coeffs = tabula_stark::rap::ef4::ef4_coeffs(opening.cumsum_final);
        rap_folder.finalize_cumsum(coeffs.map(EF4::from));

        if rap_folder.accumulator() * selectors.inv_vanishing != quotient {
            return Err(ood_mismatch.to_string());
        }
    }
    Ok(())
}

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

#[cfg(test)]
mod tests {
    use p3_field::PrimeCharacteristicRing;
    use p3_koala_bear::KoalaBear;
    use tabula_stark::chips::core_chips;

    use super::validate_opening_shapes;
    use crate::config::EF4;
    use crate::proof::model::ChipOpening;
    use crate::setup::metadata::ChipVerificationMetadata;

    fn base_opening() -> ChipOpening {
        ChipOpening {
            chip_id: core_chips::SMT_TABLE_PATH,
            main_local: vec![EF4::ZERO; 8],
            main_next: vec![EF4::ZERO; 8],
            perm_local: vec![],
            perm_next: vec![],
            preprocessed_local: None,
            preprocessed_next: None,
            quotient_chunks: vec![vec![EF4::ZERO]],
            degree_bits: 1,
            main_width: 8,
            perm_width: 0,
            cumsum_final: EF4::ZERO,
            log_quotient_chunks: 0,
            public_values: vec![KoalaBear::ZERO; 16],
        }
    }

    #[test]
    fn validate_opening_shapes_rejects_public_value_length_mismatch() {
        let mut opening = base_opening();
        opening.public_values.truncate(15);
        let metadata = ChipVerificationMetadata {
            main_width: 8,
            preprocessed_width: 0,
            preprocessed_next_row_columns: vec![],
            num_public_values: 16,
            interactions_per_row: 0,
        };

        let err = validate_opening_shapes(&opening, &metadata)
            .expect_err("public value length mismatch must fail");

        match err {
            crate::VerificationError::InvalidChipManifest { detail } => {
                assert!(detail.contains("public values length"));
                assert!(detail.contains("metadata length"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
