//! Batched multi-chip STARK prover with shared PCS.
//!
//! A single FRI opening proof covers all committed data (main, perm, quotient),
//! fixing the C1 soundness gap and reducing proof size by ~N×.

pub(crate) mod quotient;

use p3_challenger::{CanObserve, CanSample};
use p3_field::PrimeCharacteristicRing;
use p3_uni_stark::StarkGenericConfig;

use tabula_stark::air::statement::PublicStatement;
use tabula_witness::trace::TraceMap;

use crate::config::{EF4, TabulaStarkConfig};
use crate::keys::TabulaProvingKey;
use crate::proof::{ProveError, TabulaProof};
use crate::proof_instance::ProofInstance;
use crate::registry::ChipRegistry;

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
    // Phase 0-1: Collect chip metadata and evaluate interactions.
    let mut instance = ProofInstance::new(config, registry, pk, traces)?;

    // Phase 2-3: Commit preprocessed and main traces.
    let commitment = instance.commit_main()?;

    // Phase 4: Fiat-Shamir — observe & sample LogUp challenges.
    let mut challenger = config.initialise_challenger();
    let statement_felts = statement.to_field_elements();
    challenger.observe_slice(&statement_felts);
    if let Some(pp_c) = commitment.preprocessed {
        challenger.observe(pp_c);
    }
    challenger.observe(commitment.main);

    let logup_alpha: EF4 = challenger.sample();
    let logup_beta: EF4 = challenger.sample();

    // Phase 5: Generate permutation traces.
    let cumsum = instance.build_perm_traces([logup_alpha, logup_beta])?;
    if cumsum != EF4::ZERO {
        return Err(ProveError::LogUpImbalance {
            total: tabula_stark::rap::ef4::ef4_coeffs(cumsum),
        });
    }

    // Phases 6-11: Commit perm, compute quotients, open all.
    let sub_proof = instance.prove(&mut challenger)?;
    Ok(sub_proof.into_tabula_proof(statement))
}
