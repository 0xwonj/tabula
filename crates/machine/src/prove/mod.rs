//! Batched multi-chip STARK prover with shared PCS.
//!
//! A single FRI opening proof covers all committed data (main, perm, quotient),
//! fixing the C1 soundness gap and reducing proof size by ~N×.

mod quotient;
pub(crate) mod rap_folder;

pub(crate) use rap_folder::RapProverFolder;

use p3_baby_bear::BabyBear;
use p3_challenger::{CanObserve, CanSample, FieldChallenger};
use p3_commit::{Pcs, PolynomialSpace};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;
use p3_matrix::dense::RowMajorMatrix;
use p3_uni_stark::{StarkGenericConfig, get_log_num_quotient_chunks, get_symbolic_constraints};

use tabula_stark::air::statement::PublicStatement;
use tabula_stark::debug::evaluate_chip_interactions_only;
use tabula_witness::trace::TraceMap;

use crate::chip_ref::ChipRef;
use crate::config::{Challenger, EF4, TabulaPcs, TabulaStarkConfig};
use crate::keys::TabulaProvingKey;
use crate::permutation::generate_permutation_trace_from_interactions;
use crate::proof::{ChipOpening, ProveError, TabulaProof};
use crate::registry::ChipRegistry;

use self::quotient::ChipQuotientInfo;

/// PCS prover data type alias (module-private).
type PcsProverData = <TabulaPcs as Pcs<EF4, Challenger>>::ProverData;

/// Per-chip metadata collected before the batched PCS ceremony.
///
/// `main_trace` and `preprocessed` are `Option` so they can be moved
/// (via `.take()`) into PCS commit calls, avoiding a second clone.
struct ChipProveInfo<'a> {
    chip_ref: ChipRef<'a>,
    chip_id: tabula_stark::chips::ChipId,
    main_trace: Option<RowMajorMatrix<BabyBear>>,
    public_values: Vec<BabyBear>,
    preprocessed: Option<RowMajorMatrix<BabyBear>>,
    degree_bits: usize,
    main_width: usize,
    interactions_per_row: usize,
    inner_constraint_count: usize,
    total_constraint_count: usize,
    log_quotient_chunks: usize,
    // Populated after interaction evaluation (before perm trace generation).
    recorded_interactions: Vec<tabula_stark::debug::RecordedInteraction<BabyBear>>,
    // Populated after LogUp challenge sampling.
    perm_trace: Option<RowMajorMatrix<BabyBear>>,
    perm_width: usize,
    cumsum: EF4,
}

// ─── Public API ─────────────────────────────────────────────────────────────

/// Generate a Tabula STARK proof using batched PCS.
///
/// All chip traces are committed together in shared PCS rounds, producing
/// a single FRI opening proof.
pub fn prove_with_key(
    config: &TabulaStarkConfig,
    registry: &ChipRegistry,
    pk: &TabulaProvingKey,
    traces: &TraceMap,
    statement: PublicStatement,
) -> Result<TabulaProof, ProveError> {
    type P = TabulaPcs;
    type C = Challenger;

    let pcs = config.pcs();
    let mut challenger = config.initialise_challenger();

    // ── Phase 0: Collect per-chip metadata ───────────────────────────────

    let mut chip_infos = collect_chip_infos(registry, pk, traces)?;
    if chip_infos.is_empty() {
        return Err(ProveError::NoChips);
    }

    // ── Phase 1: Evaluate interactions (before commit, while traces are available)

    for info in &mut chip_infos {
        if info.interactions_per_row == 0 {
            continue;
        }

        let record = evaluate_chip_interactions_only(
            info.chip_ref.air(),
            info.main_trace.as_ref().expect("main trace consumed before interaction evaluation"),
            info.preprocessed.as_ref(),
            &info.public_values,
        );
        info.recorded_interactions = record.interactions;
    }

    // ── Phase 2: Commit preprocessed traces ─────────────────────────────
    //
    // Traces are moved (`.take()`) into PCS commit — no second clone.

    let pp_chip_indices: Vec<usize> = chip_infos
        .iter()
        .enumerate()
        .filter_map(|(i, info)| info.preprocessed.as_ref().map(|_| i))
        .collect();

    let (preprocessed_commitment, preprocessed_data) = if !pp_chip_indices.is_empty() {
        let pp_pairs: Vec<_> = pp_chip_indices
            .iter()
            .map(|&i| {
                let info = &mut chip_infos[i];
                let domain =
                    <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << info.degree_bits);
                (domain, info.preprocessed.take().expect("preprocessed trace already consumed"))
            })
            .collect();
        let (c, d) = <P as Pcs<EF4, C>>::commit_preprocessing(pcs, pp_pairs);
        (Some(c), Some(d))
    } else {
        (None, None)
    };

    // ── Phase 3: Commit all main traces (Round 1) ───────────────────────

    let main_pairs: Vec<_> = chip_infos
        .iter_mut()
        .map(|info| {
            let domain =
                <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << info.degree_bits);
            (domain, info.main_trace.take().expect("main trace already consumed"))
        })
        .collect();
    let (main_commitment, main_data) = <P as Pcs<EF4, C>>::commit(pcs, main_pairs);

    // ── Phase 4: Fiat-Shamir — observe & sample LogUp challenges ────────

    let statement_felts = statement.to_field_elements();
    challenger.observe_slice(&statement_felts);
    if let Some(ref pp_c) = preprocessed_commitment {
        challenger.observe(*pp_c);
    }
    challenger.observe(main_commitment);

    let logup_alpha: EF4 = challenger.sample();
    let logup_beta: EF4 = challenger.sample();
    let logup_challenges = [logup_alpha, logup_beta];

    // ── Phase 5: Generate permutation traces from recorded interactions ──

    let mut cumsum_total = EF4::ZERO;

    for info in &mut chip_infos {
        if info.interactions_per_row == 0 {
            continue;
        }

        let height = 1 << info.degree_bits;
        let (perm_trace, cumsum) = generate_permutation_trace_from_interactions(
            &info.recorded_interactions,
            height,
            logup_challenges,
        )?;

        info.perm_width = perm_trace.width();
        info.perm_trace = Some(perm_trace);
        info.cumsum = cumsum;
        cumsum_total += cumsum;
    }

    if cumsum_total != EF4::ZERO {
        return Err(ProveError::LogUpImbalance {
            total: crate::ef4::ef4_coeffs(cumsum_total),
        });
    }

    // ── Phase 6: Commit all perm traces (Round 2) ───────────────────────

    let rap_chip_indices: Vec<usize> = chip_infos
        .iter()
        .enumerate()
        .filter_map(|(i, info)| info.perm_trace.as_ref().map(|_| i))
        .collect();

    let (perm_commitment, perm_data) = if !rap_chip_indices.is_empty() {
        let perm_pairs: Vec<_> = rap_chip_indices
            .iter()
            .map(|&i| {
                let info = &mut chip_infos[i];
                let domain =
                    <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << info.degree_bits);
                (domain, info.perm_trace.take().expect("perm trace already consumed"))
            })
            .collect();
        let (c, d) = <P as Pcs<EF4, C>>::commit(pcs, perm_pairs);
        (Some(c), Some(d))
    } else {
        (None, None)
    };

    // ── Phase 7: Fiat-Shamir — observe & sample alpha ───────────────────

    if let Some(ref perm_c) = perm_commitment {
        challenger.observe(*perm_c);
    }
    let alpha: EF4 = challenger.sample_algebra_element();

    // ── Phase 8: Per-chip quotient computation ──────────────────────────

    let perm_idx_map = build_index_map(chip_infos.len(), &rap_chip_indices);
    let pp_idx_map = build_index_map(chip_infos.len(), &pp_chip_indices);

    let (all_quotient_ldes, quotient_chunk_map) = compute_chip_quotients(
        pcs,
        &chip_infos,
        &main_data,
        perm_data.as_ref(),
        preprocessed_data.as_ref(),
        &perm_idx_map,
        &pp_idx_map,
        alpha,
        logup_challenges,
    );

    // ── Phase 9: Commit all quotient LDEs (Round 3) ─────────────────────

    let (quotient_commitment, quotient_data) =
        <P as Pcs<EF4, C>>::commit_ldes(pcs, all_quotient_ldes);
    challenger.observe(quotient_commitment);

    // ── Phase 10-11: Open all commitments & extract per-chip openings ───

    let zeta: EF4 = challenger.sample_algebra_element();

    let (chip_openings, opening_proof) = open_and_extract(
        pcs,
        &mut challenger,
        &chip_infos,
        &main_data,
        &quotient_data,
        perm_data.as_ref(),
        preprocessed_data.as_ref(),
        &rap_chip_indices,
        &pp_chip_indices,
        &perm_idx_map,
        &pp_idx_map,
        &quotient_chunk_map,
        zeta,
    );

    Ok(TabulaProof {
        preprocessed_commitment,
        main_commitment,
        perm_commitment,
        quotient_commitment,
        opening_proof,
        chip_openings,
        statement,
    })
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Collect per-chip metadata from registry, proving key, and traces.
fn collect_chip_infos<'a>(
    registry: &'a ChipRegistry,
    pk: &TabulaProvingKey,
    traces: &TraceMap,
) -> Result<Vec<ChipProveInfo<'a>>, ProveError> {
    let mut infos = Vec::new();

    for chip in registry.chips() {
        let chip_id = chip.chip_id();
        let entry = match traces.get(chip_id) {
            Some(e) => e,
            None => continue,
        };

        let main_trace = &entry.main;
        let height = main_trace.height();
        if height == 0 {
            continue;
        }
        if !height.is_power_of_two() {
            return Err(ProveError::InvalidTraceHeight { chip_id, height });
        }

        let keygen = pk
            .get(chip_id)
            .ok_or(ProveError::MissingKeygenInfo { chip_id })?;
        let degree_bits = height.trailing_zeros() as usize;
        let main_width = main_trace.width();
        let interactions_per_row =
            keygen.interactions.num_sends_per_row + keygen.interactions.num_receives_per_row;

        let inner_constraint_count =
            get_symbolic_constraints(&ChipRef::new(chip.air()), keygen.preprocessed_width, entry.public_values.len()).len();
        let rap_count = if interactions_per_row > 0 {
            crate::keys::rap_constraint_count(interactions_per_row)
        } else {
            0
        };
        let total_constraint_count = inner_constraint_count + rap_count;

        let inner_log = get_log_num_quotient_chunks(
            &ChipRef::new(chip.air()),
            keygen.preprocessed_width,
            entry.public_values.len(),
            0,
        );
        let log_quotient_chunks = if interactions_per_row > 0 {
            inner_log.max(2)
        } else {
            inner_log
        };

        let mut cr = ChipRef::new(chip.air());
        if let Some(pp) = &entry.preprocessed {
            cr = cr.with_preprocessed(pp.clone());
        }

        infos.push(ChipProveInfo {
            chip_ref: cr,
            chip_id,
            main_trace: Some(main_trace.clone()),
            public_values: entry.public_values.clone(),
            preprocessed: entry.preprocessed.clone(),
            degree_bits,
            main_width,
            interactions_per_row,
            inner_constraint_count,
            total_constraint_count,
            log_quotient_chunks,
            recorded_interactions: Vec::new(),
            perm_trace: None,
            perm_width: 0,
            cumsum: EF4::ZERO,
        });
    }

    Ok(infos)
}

/// Build a mapping from chip index to committed-matrix index.
///
/// For each chip index in `committed_indices`, records its position
/// (0-based) within the committed batch. All other chip indices map to `None`.
fn build_index_map(num_chips: usize, committed_indices: &[usize]) -> Vec<Option<usize>> {
    let mut map = vec![None; num_chips];
    for (pos, &chip_idx) in committed_indices.iter().enumerate() {
        map[chip_idx] = Some(pos);
    }
    map
}

/// Phase 8: Compute quotient LDEs for every chip.
///
/// Returns the flat list of quotient LDE matrices and a per-chip map
/// of `(start_index, chunk_count)` into that list.
#[allow(clippy::too_many_arguments)]
fn compute_chip_quotients(
    pcs: &TabulaPcs,
    chip_infos: &[ChipProveInfo<'_>],
    main_data: &PcsProverData,
    perm_data: Option<&PcsProverData>,
    preprocessed_data: Option<&PcsProverData>,
    perm_idx_map: &[Option<usize>],
    pp_idx_map: &[Option<usize>],
    alpha: EF4,
    logup_challenges: [EF4; 2],
) -> (Vec<RowMajorMatrix<BabyBear>>, Vec<(usize, usize)>) {
    type P = TabulaPcs;
    type C = Challenger;

    let mut all_quotient_ldes: Vec<RowMajorMatrix<BabyBear>> = Vec::new();
    let mut quotient_chunk_map: Vec<(usize, usize)> = Vec::new();

    for (i, info) in chip_infos.iter().enumerate() {
        let degree_bits = info.degree_bits;
        let trace_domain =
            <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << degree_bits);
        let log_q = info.log_quotient_chunks;
        let num_q_chunks = 1 << log_q;
        let q_domain =
            trace_domain.create_disjoint_domain(1 << (degree_bits + log_q));

        let main_on_q =
            <P as Pcs<EF4, C>>::get_evaluations_on_domain(pcs, main_data, i, q_domain);

        let perm_on_q = perm_idx_map[i].map(|idx| {
            <P as Pcs<EF4, C>>::get_evaluations_on_domain(
                pcs,
                perm_data.expect("perm data missing for chip with perm index"),
                idx,
                q_domain,
            )
        });

        let pp_on_q = pp_idx_map[i].map(|idx| {
            <P as Pcs<EF4, C>>::get_evaluations_on_domain_no_random(
                pcs,
                preprocessed_data.expect("preprocessed data missing for chip with pp index"),
                idx,
                q_domain,
            )
        });

        let quotient_values = if info.interactions_per_row > 0 {
            let chip_qi = ChipQuotientInfo {
                main_width: info.main_width,
                inner_constraint_count: info.inner_constraint_count,
                total_constraint_count: info.total_constraint_count,
                cumsum_final: info.cumsum,
            };
            quotient::compute_quotient_rap(
                &info.chip_ref,
                &main_on_q,
                perm_on_q.as_ref().expect("perm trace missing for chip with interactions"),
                pp_on_q.as_ref(),
                &info.public_values,
                trace_domain,
                q_domain,
                alpha,
                logup_challenges,
                &chip_qi,
            )
        } else {
            quotient::compute_quotient_standard(
                &info.chip_ref,
                &main_on_q,
                pp_on_q.as_ref(),
                &info.public_values,
                trace_domain,
                q_domain,
                alpha,
                info.inner_constraint_count,
            )
        };

        let flat = RowMajorMatrix::new_col(quotient_values).flatten_to_base();
        let sub_evals = q_domain.split_evals(num_q_chunks, flat);
        let sub_domains = q_domain.split_domains(num_q_chunks);
        let ldes = <P as Pcs<EF4, C>>::get_quotient_ldes(
            pcs,
            sub_domains.into_iter().zip(sub_evals),
            num_q_chunks,
        );

        let start = all_quotient_ldes.len();
        let count = ldes.len();
        all_quotient_ldes.extend(ldes);
        quotient_chunk_map.push((start, count));
    }

    (all_quotient_ldes, quotient_chunk_map)
}

/// Phases 10-11: Build PCS opening rounds, run FRI, and extract per-chip openings.
///
/// Returns the per-chip `ChipOpening` list and the single FRI opening proof.
#[allow(clippy::too_many_arguments)]
fn open_and_extract(
    pcs: &TabulaPcs,
    challenger: &mut Challenger,
    chip_infos: &[ChipProveInfo<'_>],
    main_data: &PcsProverData,
    quotient_data: &PcsProverData,
    perm_data: Option<&PcsProverData>,
    preprocessed_data: Option<&PcsProverData>,
    rap_chip_indices: &[usize],
    pp_chip_indices: &[usize],
    perm_idx_map: &[Option<usize>],
    pp_idx_map: &[Option<usize>],
    quotient_chunk_map: &[(usize, usize)],
    zeta: EF4,
) -> (Vec<ChipOpening>, <TabulaPcs as Pcs<EF4, Challenger>>::Proof) {
    type P = TabulaPcs;
    type C = Challenger;

    // ── Phase 10: Build opening rounds & open all commitments ───────────

    let zeta_pair = |degree_bits: usize| -> Vec<EF4> {
        let domain = <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << degree_bits);
        let zeta_next = domain.next_point(zeta).expect("domain has no next point for zeta");
        vec![zeta, zeta_next]
    };

    let mut rounds = Vec::new();

    // Round 0: main traces — each matrix opened at [zeta, zeta_next]
    let main_points: Vec<Vec<EF4>> = chip_infos.iter().map(|i| zeta_pair(i.degree_bits)).collect();
    rounds.push((main_data, main_points));

    // Round 1: quotient chunks — each matrix opened at [zeta]
    let total_q_matrices: usize = quotient_chunk_map.iter().map(|(_, n)| n).sum();
    rounds.push((quotient_data, vec![vec![zeta]; total_q_matrices]));

    // Round 2: perm traces — each matrix opened at [zeta, zeta_next]
    if let Some(perm_d) = perm_data {
        let pts: Vec<Vec<EF4>> = rap_chip_indices.iter().map(|&i| zeta_pair(chip_infos[i].degree_bits)).collect();
        rounds.push((perm_d, pts));
    }

    // Round 3: preprocessed traces — each matrix opened at [zeta, zeta_next]
    if let Some(pp_d) = preprocessed_data {
        let pts: Vec<Vec<EF4>> = pp_chip_indices.iter().map(|&i| zeta_pair(chip_infos[i].degree_bits)).collect();
        rounds.push((pp_d, pts));
    }

    let (opened_values, opening_proof) =
        <P as Pcs<EF4, C>>::open(pcs, rounds, challenger);

    // ── Phase 11: Extract per-chip openings ─────────────────────────────
    // opened_values[round][matrix][point] = Vec<EF4>
    let main_ov = &opened_values[0];
    let quot_ov = &opened_values[1];
    let perm_round_idx = if perm_data.is_some() { Some(2) } else { None };
    let pp_round_idx = if preprocessed_data.is_some() {
        Some(if perm_data.is_some() { 3 } else { 2 })
    } else {
        None
    };

    let chip_openings = chip_infos.iter().enumerate().map(|(i, info)| {
        let (perm_local, perm_next) = match (perm_idx_map[i], perm_round_idx) {
            (Some(idx), Some(r)) => (opened_values[r][idx][0].clone(), opened_values[r][idx][1].clone()),
            _ => (vec![], vec![]),
        };
        let (preprocessed_local, preprocessed_next) = match (pp_idx_map[i], pp_round_idx) {
            (Some(idx), Some(r)) => (Some(opened_values[r][idx][0].clone()), Some(opened_values[r][idx][1].clone())),
            _ => (None, None),
        };
        let (q_start, q_count) = quotient_chunk_map[i];
        ChipOpening {
            chip_id: info.chip_id,
            main_local: main_ov[i][0].clone(),
            main_next: main_ov[i][1].clone(),
            perm_local,
            perm_next,
            preprocessed_local,
            preprocessed_next,
            quotient_chunks: (q_start..q_start + q_count).map(|qi| quot_ov[qi][0].clone()).collect(),
            degree_bits: info.degree_bits,
            main_width: info.main_width,
            perm_width: info.perm_width,
            cumsum_final: info.cumsum,
            log_quotient_chunks: info.log_quotient_chunks,
            public_values: info.public_values.clone(),
        }
    }).collect();

    (chip_openings, opening_proof)
}
