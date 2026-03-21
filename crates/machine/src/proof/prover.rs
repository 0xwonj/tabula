use std::collections::{BTreeMap, BTreeSet};

use p3_field::PrimeCharacteristicRing;
use rayon::prelude::*;

use tabula_stark::air::interaction::BusId;
use tabula_stark::trace::TraceMap;

use crate::config::{Challenger, EF4};
use crate::machine::TabulaMachine;
use crate::proof::instance::{MainCommitment, ProofInstance, SubProof};
use crate::proof::transcript::MachineTranscript;
use crate::proof::types::{
    ColumnIdentity, ColumnProofEntry, MachineProofInput, ProofTier, ProveError, SubProofEnvelope,
    TabulaProof, check_cross_proof_bus_balance,
};
use crate::setup::keys::compute_external_buses;
use crate::setup::registry::ChipRegistry;
use crate::setup::types::{MachineSetup, ProofSetups, ProofTraces};

/// Borrowed proving facade over a configured [`TabulaMachine`].
///
/// This view exposes proving-only operations while sharing the machine's
/// immutable setup and configuration.
#[derive(Clone, Copy, Debug)]
pub struct Prover<'a> {
    setup: &'a MachineSetup,
}

impl TabulaMachine {
    /// Create a borrowed proving view over this machine.
    #[must_use]
    pub fn prover(&self) -> Prover<'_> {
        self.setup().prover()
    }
}

impl MachineSetup {
    /// Create a borrowed proving view over this setup.
    #[must_use]
    pub fn prover(&self) -> Prover<'_> {
        Prover { setup: self }
    }
}

impl<'a> Prover<'a> {
    /// Generate a multi-proof from traces and bundled column identities.
    pub fn prove(&self, input: MachineProofInput) -> Result<TabulaProof, ProveError> {
        self.prove_inner(input)
    }

    fn prove_inner(self, input: MachineProofInput) -> Result<TabulaProof, ProveError> {
        let config = self.setup.config();
        let proof_setups = self.setup.proof_setups();

        let external_buses = compute_external_buses(
            std::iter::once(&proof_setups.execution.proving_key)
                .chain(proof_setups.columns.iter().map(|(_, s)| &s.proving_key))
                .chain(std::iter::once(&proof_setups.root.proving_key)),
        );

        let MachineProofInput {
            traces,
            statement,
            statement_digest,
        } = input;

        let (inputs, num_cols) = Self::assemble_instance_inputs(proof_setups, traces)?;

        let mut instances: Vec<LabeledInstance<'_>> = inputs
            .into_par_iter()
            .map(|input| {
                let instance =
                    ProofInstance::new(config, input.registry, input.proving_key, input.trace_map)?;
                Ok(LabeledInstance {
                    tier: input.tier,
                    identity: input.identity,
                    instance,
                })
            })
            .collect::<Result<Vec<_>, ProveError>>()?;

        let commitments: Vec<MainCommitment> = instances
            .par_iter_mut()
            .map(|li| li.instance.commit_main())
            .collect::<Result<Vec<_>, ProveError>>()?;

        let mut transcript = MachineTranscript::new(config);
        transcript.observe_statement_binding(&statement, &statement_digest);
        for commitment in &commitments {
            transcript.observe_main_commitment(commitment);
        }

        let challenges = transcript.sample_logup_challenges();

        instances
            .par_iter_mut()
            .try_for_each(|li| li.instance.build_perm_traces(challenges).map(|_| ()))?;

        instances.par_iter().try_for_each(|li| {
            Self::check_internal_balance(&li.instance, li.tier, &external_buses)
        })?;

        let all_external: Vec<BTreeMap<BusId, EF4>> = instances
            .iter()
            .map(|li| extract_external_cumsums(&li.instance, &external_buses))
            .collect();
        check_cross_proof_bus_balance(all_external.iter())
            .map_err(|(bus_id, total)| ProveError::CrossProofBusImbalance { bus_id, total })?;

        let challengers: Vec<Challenger> =
            (0..instances.len()).map(|_| transcript.fork()).collect();

        let all_results: Vec<_> = instances
            .into_par_iter()
            .zip(all_external.into_par_iter())
            .zip(challengers.into_par_iter())
            .map(|((li, exported), mut challenger)| {
                let sub = li.instance.prove(&mut challenger)?;
                Ok((li.tier, li.identity, exported, sub))
            })
            .collect::<Result<Vec<_>, ProveError>>()?;

        let mut results = all_results.into_iter();

        let Some((_, _, exec_cumsums, exec_sub)) = results.next() else {
            return Err(ProveError::InvalidProofInput {
                detail: "missing execution proof result".to_string(),
            });
        };
        let exec_envelope = make_envelope(ProofTier::Execution, exec_sub, exec_cumsums);

        let col_entries: Vec<ColumnProofEntry> = results
            .by_ref()
            .take(num_cols)
            .map(|(tier, identity, exported, sub)| {
                Ok(ColumnProofEntry {
                    proof: make_envelope(tier, sub, exported),
                    identity: identity.ok_or_else(|| ProveError::InvalidProofInput {
                        detail: format!("missing column identity for {tier}"),
                    })?,
                })
            })
            .collect::<Result<Vec<_>, ProveError>>()?;

        let Some((_, _, root_cumsums, root_sub)) = results.next() else {
            return Err(ProveError::InvalidProofInput {
                detail: "missing root proof result".to_string(),
            });
        };
        let root_envelope = make_envelope(ProofTier::Root, root_sub, root_cumsums);

        if results.next().is_some() {
            return Err(ProveError::InvalidProofInput {
                detail: "received extra proof results beyond execution/column/root tiers"
                    .to_string(),
            });
        }

        Ok(TabulaProof {
            execution: exec_envelope,
            columns: col_entries,
            root: root_envelope,
            statement,
            statement_digest,
        })
    }

    fn assemble_instance_inputs<'b>(
        setups: &'b ProofSetups,
        traces: ProofTraces,
    ) -> Result<(Vec<InstanceInput<'b>>, usize), ProveError> {
        let ProofTraces {
            execution: exec_traces,
            columns: col_traces,
            root: root_traces,
        } = traces;

        if col_traces.len() != setups.columns.len() {
            return Err(ProveError::InvalidProofInput {
                detail: format!(
                    "column trace count {} does not match machine setup count {}",
                    col_traces.len(),
                    setups.columns.len()
                ),
            });
        }

        let num_cols = col_traces.len();
        let mut inputs = Vec::with_capacity(2 + num_cols);
        inputs.push(InstanceInput {
            tier: ProofTier::Execution,
            identity: None,
            registry: &setups.execution.registry,
            proving_key: &setups.execution.proving_key,
            trace_map: exec_traces,
        });

        for (((table_id, col_id), setup), column_trace) in
            setups.columns.iter().zip(col_traces.into_iter())
        {
            let identity = column_trace.identity;
            if identity.table_id != table_id.0 || identity.col_id != col_id.0 {
                return Err(ProveError::InvalidProofInput {
                    detail: format!(
                        "column trace order mismatch: trace bundle has ({}, {}) but setup expects ({}, {})",
                        identity.table_id, identity.col_id, table_id.0, col_id.0
                    ),
                });
            }

            inputs.push(InstanceInput {
                tier: ProofTier::Column {
                    table_id: table_id.0,
                    col_id: col_id.0,
                },
                identity: Some(identity),
                registry: &setup.registry,
                proving_key: &setup.proving_key,
                trace_map: column_trace.trace_map,
            });
        }

        inputs.push(InstanceInput {
            tier: ProofTier::Root,
            identity: None,
            registry: &setups.root.registry,
            proving_key: &setups.root.proving_key,
            trace_map: root_traces,
        });

        Ok((inputs, num_cols))
    }

    fn check_internal_balance(
        instance: &ProofInstance<'_>,
        tier: ProofTier,
        external_buses: &BTreeSet<BusId>,
    ) -> Result<(), ProveError> {
        let cumsums = instance.cumsums_by_bus();
        for (&bus_id, &cumsum) in &cumsums {
            if external_buses.contains(&bus_id) {
                continue;
            }
            if cumsum != EF4::ZERO {
                return Err(ProveError::InternalBusImbalance {
                    tier,
                    bus_id,
                    cumsum: tabula_stark::rap::ef4::ef4_coeffs(cumsum),
                });
            }
        }
        Ok(())
    }
}

struct LabeledInstance<'a> {
    tier: ProofTier,
    identity: Option<ColumnIdentity>,
    instance: ProofInstance<'a>,
}

struct InstanceInput<'a> {
    tier: ProofTier,
    identity: Option<ColumnIdentity>,
    registry: &'a ChipRegistry,
    proving_key: &'a crate::setup::keys::TabulaProvingKey,
    trace_map: TraceMap,
}

fn make_envelope(
    tier: ProofTier,
    sub: SubProof,
    exported_cumsums: BTreeMap<BusId, EF4>,
) -> SubProofEnvelope {
    SubProofEnvelope {
        tier,
        preprocessed_commitment: sub.preprocessed_commitment,
        main_commitment: sub.main_commitment,
        perm_commitment: sub.perm_commitment,
        quotient_commitment: sub.quotient_commitment,
        opening_proof: sub.opening_proof,
        chip_openings: sub.chip_openings,
        exported_cumsums,
    }
}

fn extract_external_cumsums(
    instance: &ProofInstance<'_>,
    external_buses: &BTreeSet<BusId>,
) -> BTreeMap<BusId, EF4> {
    let all = instance.cumsums_by_bus();
    all.into_iter()
        .filter(|(bus, _)| external_buses.contains(bus))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use p3_field::PrimeCharacteristicRing;
    use p3_koala_bear::KoalaBear;
    use tabula_core::{ColId, TableId};
    use tabula_stark::trace::TraceMap;

    use super::Prover;
    use crate::TabulaMachine;
    use crate::backend::ProofColumn;
    use crate::proof::types::{ColumnIdentity, ColumnProofTrace, ProveError};
    use crate::setup::ProofTraces;
    use crate::testing::TestSsmcProofColumn;

    fn test_machine() -> TabulaMachine {
        TabulaMachine::new([
            Arc::new(TestSsmcProofColumn {
                table_id: TableId(1),
                col_id: ColId(0),
                receives_commitment: true,
            }) as Arc<dyn ProofColumn>,
            Arc::new(TestSsmcProofColumn {
                table_id: TableId(1),
                col_id: ColId(1),
                receives_commitment: true,
            }) as Arc<dyn ProofColumn>,
        ])
        .expect("machine")
    }

    fn column_trace(table_id: u32, col_id: u16) -> ColumnProofTrace {
        ColumnProofTrace {
            identity: ColumnIdentity {
                table_id,
                col_id,
                com_old: [KoalaBear::ZERO; 8],
                com_new: [KoalaBear::ZERO; 8],
            },
            trace_map: TraceMap::new(),
        }
    }

    #[test]
    fn assemble_instance_inputs_accepts_valid_ordered_column_traces() {
        let machine = test_machine();
        let traces = ProofTraces {
            execution: TraceMap::new(),
            columns: vec![column_trace(1, 0), column_trace(1, 1)],
            root: TraceMap::new(),
        };

        let (inputs, num_cols) =
            Prover::assemble_instance_inputs(machine.setup().proof_setups(), traces)
                .expect("ordered inputs");

        assert_eq!(num_cols, 2);
        assert_eq!(inputs.len(), 4);
    }

    #[test]
    fn assemble_instance_inputs_rejects_column_count_mismatch() {
        let machine = test_machine();
        let traces = ProofTraces {
            execution: TraceMap::new(),
            columns: vec![column_trace(1, 0)],
            root: TraceMap::new(),
        };

        let Err(err) = Prover::assemble_instance_inputs(machine.setup().proof_setups(), traces)
        else {
            panic!("count mismatch should fail");
        };

        match err {
            ProveError::InvalidProofInput { detail } => {
                assert!(detail.contains("column trace count"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn assemble_instance_inputs_rejects_column_order_mismatch() {
        let machine = test_machine();
        let traces = ProofTraces {
            execution: TraceMap::new(),
            columns: vec![column_trace(1, 1), column_trace(1, 0)],
            root: TraceMap::new(),
        };

        let Err(err) = Prover::assemble_instance_inputs(machine.setup().proof_setups(), traces)
        else {
            panic!("order mismatch should fail");
        };

        match err {
            ProveError::InvalidProofInput { detail } => {
                assert!(detail.contains("column trace order mismatch"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }
}
