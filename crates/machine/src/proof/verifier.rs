use std::collections::BTreeMap;

use rayon::prelude::*;
use tabula_core::{ColId, TableId};

use crate::machine::TabulaMachine;
use crate::proof::transcript::MachineTranscript;
use crate::proof::types::{TabulaProof, VerificationError, check_cross_proof_bus_balance};
use crate::setup::keys::TabulaVerifyingKey;
use crate::setup::registry::ChipRegistry;
use crate::setup::types::MachineSetup;

/// Borrowed verification facade over a configured [`TabulaMachine`].
///
/// This view exposes verification-only operations while sharing the machine's
/// immutable setup and configuration.
#[derive(Clone, Copy, Debug)]
pub struct Verifier<'a> {
    setup: &'a MachineSetup,
}

impl TabulaMachine {
    /// Create a borrowed verification view over this machine.
    #[must_use]
    pub fn verifier(&self) -> Verifier<'_> {
        self.setup().verifier()
    }
}

impl MachineSetup {
    /// Create a borrowed verification view over this setup.
    #[must_use]
    pub fn verifier(&self) -> Verifier<'_> {
        Verifier { setup: self }
    }
}

impl Verifier<'_> {
    /// Verify a multi-proof against this machine's verifier state.
    pub fn verify(&self, proof: &TabulaProof) -> Result<(), VerificationError> {
        self.verify_inner(proof)
    }

    fn verify_inner(self, proof: &TabulaProof) -> Result<(), VerificationError> {
        let config = self.setup.config();
        let proof_setups = self.setup.proof_setups();

        let mut transcript = MachineTranscript::new(config);
        transcript.observe_statement_binding(&proof.statement, &proof.statement_digest);
        transcript.observe_envelope_commitment(&proof.execution);
        for column in &proof.columns {
            transcript.observe_envelope_commitment(&column.proof);
        }
        transcript.observe_envelope_commitment(&proof.root);

        let logup_challenges = transcript.sample_logup_challenges();

        let setup_index: BTreeMap<_, _> = proof_setups
            .columns
            .iter()
            .enumerate()
            .map(|(i, ((table_id, col_id), _))| ((*table_id, *col_id), i))
            .collect();

        let mut verify_tasks: Vec<(&ChipRegistry, &TabulaVerifyingKey, _)> =
            Vec::with_capacity(2 + proof.columns.len());

        verify_tasks.push((
            &proof_setups.execution.registry,
            &proof_setups.execution.verifying_key,
            &proof.execution,
        ));

        for (index, column) in proof.columns.iter().enumerate() {
            let key = (
                TableId(column.identity.table_id),
                ColId(column.identity.col_id),
            );
            let setup_idx =
                setup_index
                    .get(&key)
                    .ok_or(VerificationError::ColumnIdentityMismatch {
                        index,
                        proof_table: column.identity.table_id,
                        proof_col: column.identity.col_id,
                    })?;
            let setup = &proof_setups.columns[*setup_idx].1;
            verify_tasks.push((&setup.registry, &setup.verifying_key, &column.proof));
        }

        verify_tasks.push((
            &proof_setups.root.registry,
            &proof_setups.root.verifying_key,
            &proof.root,
        ));

        verify_tasks
            .par_iter()
            .try_for_each(|(registry, verifying_key, envelope)| {
                let mut challenger = transcript.fork();
                crate::proof::subproof::verify_sub_proof_with_challenges(
                    config,
                    registry,
                    verifying_key,
                    &envelope.chip_openings,
                    envelope.preprocessed_commitment.clone(),
                    envelope.main_commitment.clone(),
                    envelope.perm_commitment.clone(),
                    envelope.quotient_commitment.clone(),
                    &envelope.opening_proof,
                    logup_challenges,
                    &mut challenger,
                )
            })?;

        let all_maps = std::iter::once(&proof.execution.exported_cumsums)
            .chain(
                proof
                    .columns
                    .iter()
                    .map(|column| &column.proof.exported_cumsums),
            )
            .chain(std::iter::once(&proof.root.exported_cumsums));

        check_cross_proof_bus_balance(all_maps)
            .map_err(|(bus_id, total)| VerificationError::CrossProofBusImbalance { bus_id, total })
    }
}
