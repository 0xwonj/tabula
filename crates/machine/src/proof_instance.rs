//! ProofInstance: phased STARK prover abstraction.
//!
//! Encapsulates a chip set with independent PCS, providing phase-level
//! methods for the batched proving protocol. Each proof instance owns its
//! own chip subset; the multi-proof orchestrator creates C+2 instances
//! sharing a synchronized Fiat-Shamir transcript.

use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;
use p3_challenger::{CanObserve, FieldChallenger};
use p3_commit::{Pcs, PolynomialSpace};
use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;
use p3_matrix::dense::RowMajorMatrix;
use p3_uni_stark::{StarkGenericConfig, get_log_num_quotient_chunks, get_symbolic_constraints};
use rayon::prelude::*;

use tabula_stark::air::interaction::BusId;
use tabula_stark::debug::evaluate_chip_interactions_only;
use tabula_witness::trace::TraceMap;

use crate::chip_ref::ChipRef;
use crate::config::{
    Challenger, EF4, PcsCommitment, PcsOpeningProof, TabulaPcs, TabulaStarkConfig,
};
use crate::keys::TabulaProvingKey;
use crate::proof::{ChipOpening, ProveError};
use crate::prove::quotient;
use crate::registry::ChipRegistry;
use tabula_stark::permutation::generate_permutation_trace_from_interactions;

/// PCS prover data type alias.
type PcsProverData = <TabulaPcs as Pcs<EF4, Challenger>>::ProverData;

// ─── Output Types ──────────────────────────────────────────────────────────

/// PCS commitments from the main trace commit phase.
///
/// The orchestrator observes these into the Fiat-Shamir transcript before
/// sampling LogUp challenges.
pub(crate) struct MainCommitment {
    /// Commitment to preprocessed traces (`None` if no chip requires preprocessing).
    pub preprocessed: Option<PcsCommitment>,
    /// Commitment to all chip main traces.
    pub main: PcsCommitment,
}

/// Output of a completed proof instance.
///
/// Contains all PCS commitments, opening proof, and per-chip evaluations
/// for one proof instance.
pub(crate) struct SubProof {
    pub preprocessed_commitment: Option<PcsCommitment>,
    pub main_commitment: PcsCommitment,
    pub perm_commitment: Option<PcsCommitment>,
    pub quotient_commitment: PcsCommitment,
    pub opening_proof: PcsOpeningProof,
    pub chip_openings: Vec<ChipOpening>,
}

// ─── ProofInstance ─────────────────────────────────────────────────────────

/// A self-contained proving unit with phase-level methods.
///
/// Wraps a set of chips and accumulates PCS state across proving phases.
/// The orchestrator (`prove_with_key`) manages the Fiat-Shamir transcript
/// between phases, enabling future extension to multiple synchronized
/// proof instances (sharding).
///
/// # Phases
///
/// 1. **`new()`** — Phase 0-1: collect chip metadata, evaluate interactions
/// 2. **`commit_main()`** — Phase 2-3: commit preprocessed + main traces
/// 3. *(orchestrator observes commitments, samples LogUp challenges)*
/// 4. **`build_perm_traces()`** — Phase 5: generate permutation traces
/// 5. *(orchestrator checks cumsum balance)*
/// 6. **`prove()`** — Phase 6-11: commit perm, quotients, open all
pub(crate) struct ProofInstance<'a> {
    config: &'a TabulaStarkConfig,
    chip_infos: Vec<ChipProveInfo<'a>>,
    /// Indices of chips with preprocessed traces (populated by `commit_main`).
    pp_chip_indices: Vec<usize>,
    /// Preprocessed trace PCS commitment (populated by `commit_main`).
    preprocessed_commitment: Option<PcsCommitment>,
    /// Preprocessed trace PCS prover data (populated by `commit_main`).
    preprocessed_data: Option<PcsProverData>,
    /// Main trace PCS commitment (populated by `commit_main`).
    main_commitment: Option<PcsCommitment>,
    /// Main trace PCS prover data (populated by `commit_main`).
    main_data: Option<PcsProverData>,
    /// LogUp challenges (populated by `build_perm_traces`).
    logup_challenges: Option<[EF4; 2]>,
}

// Compile-time assertion: ProofInstance must be Send for rayon parallelism.
const _: () = {
    fn _assert_send() {
        fn check<T: Send>() {}
        check::<ProofInstance<'_>>();
    }
};

impl<'a> ProofInstance<'a> {
    /// Phase 0-1: Collect per-chip metadata and evaluate interactions.
    ///
    /// Evaluates LogUp interactions for all chips while main traces are
    /// still available (before PCS commit consumes them).
    pub fn new(
        config: &'a TabulaStarkConfig,
        registry: &'a ChipRegistry,
        pk: &TabulaProvingKey,
        mut traces: TraceMap,
    ) -> Result<Self, ProveError> {
        let mut chip_infos = collect_chip_infos(registry, pk, &mut traces)?;
        if chip_infos.is_empty() {
            return Err(ProveError::NoChips);
        }

        // Phase 1: Evaluate interactions before commit consumes traces.
        for info in &mut chip_infos {
            if info.interactions_per_row == 0 {
                continue;
            }
            let record = evaluate_chip_interactions_only(
                info.chip_ref.air(),
                info.main_trace
                    .as_ref()
                    .expect("main trace consumed before interaction evaluation"),
                info.preprocessed.as_ref(),
                &info.public_values,
            );
            info.recorded_interactions = record.interactions;
        }

        Ok(Self {
            config,
            chip_infos,
            pp_chip_indices: Vec::new(),
            preprocessed_commitment: None,
            preprocessed_data: None,
            main_commitment: None,
            main_data: None,
            logup_challenges: None,
        })
    }

    /// Phase 2-3: Commit preprocessed and main traces.
    ///
    /// Returns PCS commitments for the orchestrator to observe into the
    /// Fiat-Shamir transcript. Traces are moved into PCS commit (no clone).
    pub fn commit_main(&mut self) -> Result<MainCommitment, ProveError> {
        let pcs = self.config.pcs();
        type P = TabulaPcs;
        type C = Challenger;

        // Phase 2: Commit preprocessed traces.
        let pp_chip_indices: Vec<usize> = self
            .chip_infos
            .iter()
            .enumerate()
            .filter_map(|(i, info)| info.preprocessed.as_ref().map(|_| i))
            .collect();

        let (preprocessed_commitment, preprocessed_data) = if !pp_chip_indices.is_empty() {
            let pp_pairs: Vec<_> = pp_chip_indices
                .iter()
                .map(|&i| {
                    let info = &mut self.chip_infos[i];
                    let domain =
                        <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << info.degree_bits);
                    (
                        domain,
                        info.preprocessed
                            .take()
                            .expect("preprocessed trace already consumed"),
                    )
                })
                .collect();
            let (c, d) = <P as Pcs<EF4, C>>::commit_preprocessing(pcs, pp_pairs);
            (Some(c), Some(d))
        } else {
            (None, None)
        };

        // Phase 3: Commit all main traces.
        let main_pairs: Vec<_> = self
            .chip_infos
            .iter_mut()
            .map(|info| {
                let domain =
                    <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << info.degree_bits);
                (
                    domain,
                    info.main_trace.take().expect("main trace already consumed"),
                )
            })
            .collect();
        let (main_commitment, main_data) = <P as Pcs<EF4, C>>::commit(pcs, main_pairs);

        self.pp_chip_indices = pp_chip_indices;
        self.preprocessed_commitment = preprocessed_commitment;
        self.preprocessed_data = preprocessed_data;
        self.main_commitment = Some(main_commitment);
        self.main_data = Some(main_data);

        Ok(MainCommitment {
            preprocessed: preprocessed_commitment,
            main: main_commitment,
        })
    }

    /// Phase 5: Generate permutation traces from recorded interactions.
    ///
    /// Returns the total cumulative sum across all chips in this instance.
    /// The orchestrator checks that the sum across all instances is zero.
    pub fn build_perm_traces(&mut self, challenges: [EF4; 2]) -> Result<EF4, ProveError> {
        self.logup_challenges = Some(challenges);

        // Parallelize per-chip perm trace generation (each chip is independent).
        self.chip_infos
            .par_iter_mut()
            .try_for_each(|info| -> Result<(), ProveError> {
                if info.interactions_per_row == 0 {
                    return Ok(());
                }
                let height = 1 << info.degree_bits;
                let output = generate_permutation_trace_from_interactions(
                    &info.recorded_interactions,
                    height,
                    challenges,
                )?;
                info.perm_width = output.trace.width();
                info.perm_trace = Some(output.trace);
                info.cumsum = output.cumsum;
                info.cumsums_by_bus = output.cumsums_by_bus;
                Ok(())
            })?;

        // Aggregate cumsums sequentially (cheap summation).
        let cumsum_total = self
            .chip_infos
            .iter()
            .map(|info| info.cumsum)
            .fold(EF4::ZERO, |acc, c| acc + c);

        Ok(cumsum_total)
    }

    /// Per-bus cumulative sums aggregated across all chips in this instance.
    ///
    /// Available after [`build_perm_traces()`]. Used by the sharded prover to
    /// classify internal (must be zero) vs external (exported) bus cumsums.
    pub fn cumsums_by_bus(&self) -> BTreeMap<BusId, EF4> {
        let mut totals: BTreeMap<BusId, EF4> = BTreeMap::new();
        for info in &self.chip_infos {
            for (&bus, &cs) in &info.cumsums_by_bus {
                *totals.entry(bus).or_insert(EF4::ZERO) += cs;
            }
        }
        totals
    }

    /// Phases 6-11: Commit perm traces, compute quotients, open all.
    ///
    /// The `challenger` must have already observed main commitments and
    /// sampled LogUp challenges. This method observes the perm commitment,
    /// samples alpha, and completes the proving protocol.
    pub fn prove(mut self, challenger: &mut Challenger) -> Result<SubProof, ProveError> {
        let pcs = self.config.pcs();
        let logup_challenges = self
            .logup_challenges
            .expect("build_perm_traces must be called before prove");
        let main_data = self
            .main_data
            .take()
            .expect("commit_main must be called before prove");
        let main_commitment = self
            .main_commitment
            .expect("commit_main must be called before prove");

        type P = TabulaPcs;
        type C = Challenger;

        // ── Phase 6: Commit perm traces ─────────────────────────────────

        let rap_chip_indices: Vec<usize> = self
            .chip_infos
            .iter()
            .enumerate()
            .filter_map(|(i, info)| info.perm_trace.as_ref().map(|_| i))
            .collect();

        let (perm_commitment, perm_data) = if !rap_chip_indices.is_empty() {
            let perm_pairs: Vec<_> = rap_chip_indices
                .iter()
                .map(|&i| {
                    let info = &mut self.chip_infos[i];
                    let domain =
                        <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << info.degree_bits);
                    (
                        domain,
                        info.perm_trace.take().expect("perm trace already consumed"),
                    )
                })
                .collect();
            let (c, d) = <P as Pcs<EF4, C>>::commit(pcs, perm_pairs);
            (Some(c), Some(d))
        } else {
            (None, None)
        };

        // ── Phase 7: Observe perm commitment, sample alpha ──────────────

        if let Some(ref perm_c) = perm_commitment {
            challenger.observe(*perm_c);
        }
        let alpha: EF4 = challenger.sample_algebra_element();

        // ── Phase 8: Per-chip quotient computation ──────────────────────

        let num_chips = self.chip_infos.len();
        let perm_idx_map = build_index_map(num_chips, &rap_chip_indices);
        let pp_idx_map = build_index_map(num_chips, &self.pp_chip_indices);
        let pp_chip_indices = std::mem::take(&mut self.pp_chip_indices);

        let committed = CommittedData {
            pcs,
            main_data: &main_data,
            perm_data: perm_data.as_ref(),
            preprocessed_data: self.preprocessed_data.as_ref(),
            rap_chip_indices,
            pp_chip_indices,
            perm_idx_map,
            pp_idx_map,
        };

        let (all_quotient_ldes, quotient_chunk_map) =
            compute_chip_quotients(&committed, &self.chip_infos, alpha, logup_challenges);

        // ── Phase 9: Commit quotient LDEs ───────────────────────────────

        let (quotient_commitment, quotient_data) =
            <P as Pcs<EF4, C>>::commit_ldes(pcs, all_quotient_ldes);
        challenger.observe(quotient_commitment);

        // ── Phases 10-11: Open all commitments & extract openings ────────

        let zeta: EF4 = challenger.sample_algebra_element();

        let (chip_openings, opening_proof) = open_and_extract(
            &committed,
            challenger,
            &self.chip_infos,
            &quotient_data,
            &quotient_chunk_map,
            zeta,
        );

        Ok(SubProof {
            preprocessed_commitment: self.preprocessed_commitment,
            main_commitment,
            perm_commitment,
            quotient_commitment,
            opening_proof,
            chip_openings,
        })
    }
}

// ─── Internal Types ────────────────────────────────────────────────────────

/// Committed PCS state shared between quotient computation and FRI opening.
///
/// Bundles PCS prover data with chip-to-matrix index maps, avoiding
/// parameter proliferation across proving phases 8-11.
struct CommittedData<'a> {
    pcs: &'a TabulaPcs,
    main_data: &'a PcsProverData,
    perm_data: Option<&'a PcsProverData>,
    preprocessed_data: Option<&'a PcsProverData>,
    /// Which chips have permutation traces (indices into chip_infos).
    rap_chip_indices: Vec<usize>,
    /// Which chips have preprocessed traces (indices into chip_infos).
    pp_chip_indices: Vec<usize>,
    /// Maps chip index → committed perm matrix index.
    perm_idx_map: Vec<Option<usize>>,
    /// Maps chip index → committed preprocessed matrix index.
    pp_idx_map: Vec<Option<usize>>,
}

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
    /// Populated after interaction evaluation (before perm trace generation).
    recorded_interactions: Vec<tabula_stark::debug::RecordedInteraction<BabyBear>>,
    /// Populated after LogUp challenge sampling.
    perm_trace: Option<RowMajorMatrix<BabyBear>>,
    perm_width: usize,
    cumsum: EF4,
    /// Per-bus cumulative sums for sharding: maps BusId → cumsum contribution.
    cumsums_by_bus: BTreeMap<BusId, EF4>,
}

// ─── Helpers ───────────────────────────────────────────────────────────────

/// Collect per-chip metadata from registry, proving key, and traces.
///
/// Drains matching entries from `traces`, transferring ownership of trace
/// matrices into `ChipProveInfo` without cloning.
fn collect_chip_infos<'a>(
    registry: &'a ChipRegistry,
    pk: &TabulaProvingKey,
    traces: &mut TraceMap,
) -> Result<Vec<ChipProveInfo<'a>>, ProveError> {
    let mut infos = Vec::new();

    for chip in registry.chips() {
        let chip_id = chip.chip_id();
        let Some(entry) = traces.remove(chip_id) else {
            continue;
        };

        let height = entry.main.height();
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
        let main_width = entry.main.width();
        let interactions_per_row =
            keygen.interactions.num_sends_per_row + keygen.interactions.num_receives_per_row;

        let inner_constraint_count = get_symbolic_constraints(
            &ChipRef::new(chip.air()),
            keygen.preprocessed_width,
            entry.public_values.len(),
        )
        .len();
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
        if let Some(ref pp) = entry.preprocessed {
            cr = cr.with_preprocessed(pp.clone());
        }

        infos.push(ChipProveInfo {
            chip_ref: cr,
            chip_id,
            main_trace: Some(entry.main),
            public_values: entry.public_values,
            preprocessed: entry.preprocessed,
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
            cumsums_by_bus: BTreeMap::new(),
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
fn compute_chip_quotients(
    ctx: &CommittedData<'_>,
    chip_infos: &[ChipProveInfo<'_>],
    alpha: EF4,
    logup_challenges: [EF4; 2],
) -> (Vec<RowMajorMatrix<BabyBear>>, Vec<(usize, usize)>) {
    let pcs = ctx.pcs;
    type P = TabulaPcs;
    type C = Challenger;

    // Compute per-chip quotient LDEs in parallel.
    let per_chip: Vec<Vec<RowMajorMatrix<BabyBear>>> = chip_infos
        .par_iter()
        .enumerate()
        .map(|(i, info)| {
            let degree_bits = info.degree_bits;
            let trace_domain = <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << degree_bits);
            let log_q = info.log_quotient_chunks;
            let num_q_chunks = 1 << log_q;
            let q_domain = trace_domain.create_disjoint_domain(1 << (degree_bits + log_q));

            let main_on_q =
                <P as Pcs<EF4, C>>::get_evaluations_on_domain(pcs, ctx.main_data, i, q_domain);

            let perm_on_q = ctx.perm_idx_map[i].map(|idx| {
                <P as Pcs<EF4, C>>::get_evaluations_on_domain(
                    pcs,
                    ctx.perm_data
                        .expect("perm data missing for chip with perm index"),
                    idx,
                    q_domain,
                )
            });

            let pp_on_q = ctx.pp_idx_map[i].map(|idx| {
                <P as Pcs<EF4, C>>::get_evaluations_on_domain_no_random(
                    pcs,
                    ctx.preprocessed_data
                        .expect("preprocessed data missing for chip with pp index"),
                    idx,
                    q_domain,
                )
            });

            let quotient_values = if info.interactions_per_row > 0 {
                let chip_qi = quotient::ChipQuotientInfo {
                    main_width: info.main_width,
                    inner_constraint_count: info.inner_constraint_count,
                    total_constraint_count: info.total_constraint_count,
                    cumsum_final: info.cumsum,
                };
                quotient::compute_quotient_rap(
                    &info.chip_ref,
                    &main_on_q,
                    perm_on_q
                        .as_ref()
                        .expect("perm trace missing for chip with interactions"),
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
            <P as Pcs<EF4, C>>::get_quotient_ldes(
                pcs,
                sub_domains.into_iter().zip(sub_evals),
                num_q_chunks,
            )
        })
        .collect();

    // Sequential assembly: start indices depend on previous chunks.
    let mut all_quotient_ldes: Vec<RowMajorMatrix<BabyBear>> = Vec::new();
    let mut quotient_chunk_map: Vec<(usize, usize)> = Vec::new();
    for ldes in per_chip {
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
fn open_and_extract(
    ctx: &CommittedData<'_>,
    challenger: &mut Challenger,
    chip_infos: &[ChipProveInfo<'_>],
    quotient_data: &PcsProverData,
    quotient_chunk_map: &[(usize, usize)],
    zeta: EF4,
) -> (Vec<ChipOpening>, PcsOpeningProof) {
    let pcs = ctx.pcs;
    type P = TabulaPcs;
    type C = Challenger;

    // ── Phase 10: Build opening rounds & open all commitments ───────────

    let zeta_pair = |degree_bits: usize| -> Vec<EF4> {
        let domain = <P as Pcs<EF4, C>>::natural_domain_for_degree(pcs, 1 << degree_bits);
        let zeta_next = domain
            .next_point(zeta)
            .expect("domain has no next point for zeta");
        vec![zeta, zeta_next]
    };

    let mut rounds = Vec::new();

    // Round 0: main traces — each matrix opened at [zeta, zeta_next]
    let main_points: Vec<Vec<EF4>> = chip_infos
        .iter()
        .map(|i| zeta_pair(i.degree_bits))
        .collect();
    rounds.push((ctx.main_data, main_points));

    // Round 1: quotient chunks — each matrix opened at [zeta]
    let total_q_matrices: usize = quotient_chunk_map.iter().map(|(_, n)| n).sum();
    rounds.push((quotient_data, vec![vec![zeta]; total_q_matrices]));

    // Round 2: perm traces — each matrix opened at [zeta, zeta_next]
    if let Some(perm_d) = ctx.perm_data {
        let pts: Vec<Vec<EF4>> = ctx
            .rap_chip_indices
            .iter()
            .map(|&i| zeta_pair(chip_infos[i].degree_bits))
            .collect();
        rounds.push((perm_d, pts));
    }

    // Round 3: preprocessed traces — each matrix opened at [zeta, zeta_next]
    if let Some(pp_d) = ctx.preprocessed_data {
        let pts: Vec<Vec<EF4>> = ctx
            .pp_chip_indices
            .iter()
            .map(|&i| zeta_pair(chip_infos[i].degree_bits))
            .collect();
        rounds.push((pp_d, pts));
    }

    let (opened_values, opening_proof) = <P as Pcs<EF4, C>>::open(pcs, rounds, challenger);

    // ── Phase 11: Extract per-chip openings ─────────────────────────────
    // opened_values[round][matrix][point] = Vec<EF4>
    let main_ov = &opened_values[0];
    let quot_ov = &opened_values[1];
    let perm_round_idx = if ctx.perm_data.is_some() {
        Some(2)
    } else {
        None
    };
    let pp_round_idx = if ctx.preprocessed_data.is_some() {
        Some(if ctx.perm_data.is_some() { 3 } else { 2 })
    } else {
        None
    };

    let chip_openings = chip_infos
        .iter()
        .enumerate()
        .map(|(i, info)| {
            let (perm_local, perm_next) = match (ctx.perm_idx_map[i], perm_round_idx) {
                (Some(idx), Some(r)) => (
                    opened_values[r][idx][0].clone(),
                    opened_values[r][idx][1].clone(),
                ),
                _ => (vec![], vec![]),
            };
            let (preprocessed_local, preprocessed_next) = match (ctx.pp_idx_map[i], pp_round_idx) {
                (Some(idx), Some(r)) => (
                    Some(opened_values[r][idx][0].clone()),
                    Some(opened_values[r][idx][1].clone()),
                ),
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
                quotient_chunks: (q_start..q_start + q_count)
                    .map(|qi| quot_ov[qi][0].clone())
                    .collect(),
                degree_bits: info.degree_bits,
                main_width: info.main_width,
                perm_width: info.perm_width,
                cumsum_final: info.cumsum,
                log_quotient_chunks: info.log_quotient_chunks,
                public_values: info.public_values.clone(),
            }
        })
        .collect();

    (chip_openings, opening_proof)
}
