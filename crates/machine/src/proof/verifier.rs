use std::collections::BTreeMap;

use rayon::prelude::*;

use crate::input::ColumnSlotKey;
use crate::proof::errors::VerificationError;
use crate::proof::model::{TabulaProof, check_cross_proof_bus_balance};
use crate::proof::transcript::MachineTranscript;
use crate::setup::metadata::TierVerificationMetadata;
use crate::setup::registry::ChipRegistry;
use crate::setup::topology::MachineTopology;

/// Borrowed verification facade over a configured [`TabulaMachine`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct Verifier<'a> {
    topology: &'a MachineTopology,
}

impl Verifier<'_> {
    pub(crate) fn new(topology: &MachineTopology) -> Verifier<'_> {
        Verifier { topology }
    }

    /// Verify a multi-proof against this machine's verifier state.
    pub fn verify(self, proof: &TabulaProof) -> Result<(), VerificationError> {
        self.verify_inner(proof)
    }

    fn verify_inner(self, proof: &TabulaProof) -> Result<(), VerificationError> {
        let config = self.topology.config();
        let proof_topology = self.topology.proof_topology();

        let mut transcript = MachineTranscript::new(config);
        transcript.observe_statement_binding(&proof.statement, &proof.statement_digest);
        transcript.observe_envelope_commitment(&proof.execution);
        for column in &proof.columns {
            transcript.observe_envelope_commitment(&column.proof);
        }
        transcript.observe_envelope_commitment(&proof.root);

        let logup_challenges = transcript.sample_logup_challenges();

        let setup_index: BTreeMap<_, _> = proof_topology
            .columns
            .iter()
            .enumerate()
            .map(|(i, ((table_id, col_id), _))| {
                (
                    ColumnSlotKey {
                        table: *table_id,
                        col: *col_id,
                    },
                    i,
                )
            })
            .collect();

        let mut verify_tasks: Vec<(&ChipRegistry, &TierVerificationMetadata, _)> =
            Vec::with_capacity(2 + proof.columns.len());

        verify_tasks.push((
            &proof_topology.execution.registry,
            &proof_topology.execution.verification_metadata,
            &proof.execution,
        ));

        for (index, column) in proof.columns.iter().enumerate() {
            let setup_idx =
                setup_index
                    .get(&column.key)
                    .ok_or(VerificationError::ColumnKeyMismatch {
                        index,
                        proof_key: column.key,
                    })?;
            let topology = &proof_topology.columns[*setup_idx].1;
            verify_tasks.push((
                &topology.registry,
                &topology.verification_metadata,
                &column.proof,
            ));
        }

        verify_tasks.push((
            &proof_topology.root.registry,
            &proof_topology.root.verification_metadata,
            &proof.root,
        ));

        verify_tasks
            .par_iter()
            .try_for_each(|(registry, verification_metadata, envelope)| {
                let mut challenger = transcript.fork();
                crate::proof::subproof::verify_sub_proof_with_challenges(
                    config,
                    registry,
                    verification_metadata,
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
