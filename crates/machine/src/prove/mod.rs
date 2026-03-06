//! Multi-chip STARK prover with RAP (Randomized AIR with Preprocessing).
//!
//! For chips with LogUp interactions, generates permutation traces and proves
//! them alongside the main trace via a two-phase constraint evaluation:
//!
//! 1. **Phase 1** (inner chip): Evaluates the chip's native AIR constraints
//!    against a truncated view containing only the main trace columns.
//! 2. **Phase 2** (RAP): Evaluates permutation constraints (`phi·f = m`,
//!    cumsum transitions) against the full combined trace (main ∥ perm).
//!
//! Both phases share the same alpha-power accumulator, producing a single
//! quotient polynomial that is PCS-committed. This fixes the C1 soundness
//! gap: cumsum values are now embedded in the committed trace and cannot
//! be forged without breaking FRI.

mod pipeline;
pub(crate) mod rap_folder;

pub(crate) use rap_folder::RapProverFolder;

use p3_field::PrimeCharacteristicRing;
use p3_matrix::Matrix;

use tabula_stark::air::statement::PublicStatement;
use tabula_stark::debug::evaluate_chip_interactions_only;
use tabula_witness::trace::TraceMap;

use crate::chip_ref::ChipRef;
use crate::config::{EF4, TabulaStarkConfig};
use crate::keys::TabulaProvingKey;
use crate::permutation::{
    self, ChipTraceInfo, concat_traces, generate_permutation_trace_from_interactions,
};
use crate::proof::{ProveError, TabulaProof};
use crate::registry::ChipRegistry;

use pipeline::{prove_chip_rap, prove_chip_standard};

// ─── Registry-based prover ──────────────────────────────────────────────────

/// Generate a Tabula STARK proof using cached [`TabulaProvingKey`].
///
/// Uses dynamic dispatch via [`ChipRef`] instead of compile-time enum dispatch.
/// The proving key caches keygen info, avoiding redundant extraction on
/// repeated calls.
pub fn prove_with_key(
    config: &TabulaStarkConfig,
    registry: &ChipRegistry,
    pk: &TabulaProvingKey,
    traces: &TraceMap,
    statement: PublicStatement,
) -> Result<TabulaProof, ProveError> {
    let keygen_info = &pk.chip_info;

    // Build ChipRefs from registry, attaching preprocessed data from traces.
    let chip_refs: Vec<ChipRef<'_>> = registry
        .chips()
        .iter()
        .filter_map(|chip| {
            let id = chip.chip_id();
            let entry = traces.get(id)?;
            let mut cr = ChipRef::new(chip.air());
            if let Some(pp) = &entry.preprocessed {
                cr = cr.with_preprocessed(pp.clone());
            }
            Some(cr)
        })
        .collect();

    // Derive LogUp challenges from main trace metadata.
    let chip_infos: Vec<ChipTraceInfo> = chip_refs
        .iter()
        .map(|cr| {
            let entry = traces.get(cr.chip_id()).expect("chip trace must exist");
            ChipTraceInfo {
                trace_height: entry.main.height(),
                public_values: entry.public_values.clone(),
            }
        })
        .collect();
    let logup_challenges = permutation::derive_challenges_from_main(&chip_infos);

    let mut chip_proofs = Vec::with_capacity(chip_refs.len());
    let mut cumsum_total = EF4::ZERO;

    for cr in &chip_refs {
        let chip_id = cr.chip_id();
        let entry = traces.get(chip_id).expect("chip trace must exist");
        let main_trace = &entry.main;
        let height = main_trace.height();

        if height == 0 {
            continue;
        }

        if !height.is_power_of_two() {
            return Err(ProveError::InvalidTraceHeight { chip_id, height });
        }

        let info = keygen_info
            .get(&chip_id)
            .ok_or(ProveError::MissingKeygenInfo { chip_id })?;
        let main_width = main_trace.width();
        let interactions_per_row =
            info.interactions.num_sends_per_row + info.interactions.num_receives_per_row;

        if interactions_per_row == 0 {
            let proof_entry = prove_chip_standard(
                config,
                cr,
                main_trace,
                &entry.public_values,
                main_width,
            );
            chip_proofs.push(proof_entry);
        } else {
            let record = evaluate_chip_interactions_only(
                cr.air(),
                main_trace,
                entry.preprocessed.as_ref(),
                &entry.public_values,
            );

            let (perm_trace, cumsum) = generate_permutation_trace_from_interactions(
                &record.interactions,
                height,
                logup_challenges,
            );
            let combined = concat_traces(main_trace, &perm_trace);
            cumsum_total += cumsum;

            let proof_entry = prove_chip_rap(
                config,
                cr,
                combined,
                &entry.public_values,
                logup_challenges,
                main_width,
                perm_trace.width(),
                interactions_per_row,
                cumsum,
            );
            chip_proofs.push(proof_entry);
        }
    }

    if cumsum_total != EF4::ZERO {
        return Err(ProveError::LogUpImbalance {
            total: crate::ef4::ef4_coeffs(cumsum_total),
        });
    }

    Ok(TabulaProof {
        chip_proofs,
        logup_challenges,
        statement,
    })
}
