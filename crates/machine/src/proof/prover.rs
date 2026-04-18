use std::collections::{BTreeMap, BTreeSet};

use p3_field::PrimeCharacteristicRing;
use rayon::prelude::*;
use tabula_stark::air::interaction::BusId;
use tabula_stark::trace::TraceMap;

use crate::config::{Challenger, EF4};
use crate::input::assembly::{TierTraceBundle, build_proof_traces};
use crate::input::{ColumnSlotKey, PreparedMachineInput};
use crate::proof::errors::ProveError;
use crate::proof::instance::{MainCommitment, ProofInstance, SubProof};
use crate::proof::model::{
    ColumnProofEntry, ProofTier, SubProofEnvelope, TabulaProof, check_cross_proof_bus_balance,
};
use crate::proof::transcript::MachineTranscript;
use crate::setup::metadata::{TierProvingMetadata, compute_external_buses};
use crate::setup::registry::ChipRegistry;
use crate::setup::topology::{MachineTopology, ProofTopology};

/// Borrowed proving facade over a configured [`TabulaMachine`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct Prover<'a> {
    topology: &'a MachineTopology,
}

impl<'a> Prover<'a> {
    pub(crate) fn new(topology: &'a MachineTopology) -> Self {
        Self { topology }
    }

    /// Generate a multi-proof from prepared machine input.
    pub fn prove(self, input: PreparedMachineInput) -> Result<TabulaProof, ProveError> {
        self.prove_inner(input)
    }

    fn prove_inner(self, input: PreparedMachineInput) -> Result<TabulaProof, ProveError> {
        let config = self.topology.config();
        let proof_topology = self.topology.proof_topology();

        let external_buses = compute_external_buses(
            std::iter::once(&proof_topology.execution.proving_metadata)
                .chain(
                    proof_topology
                        .columns
                        .iter()
                        .map(|(_, topology)| &topology.proving_metadata),
                )
                .chain(std::iter::once(&proof_topology.root.proving_metadata)),
        );

        let PreparedMachineInput {
            execution,
            columns,
            root,
            binding_digest,
        } = input;
        let traces = build_proof_traces(proof_topology, execution.store, columns, root.store)?;

        let (inputs, num_cols) = Self::assemble_instance_inputs(proof_topology, traces)?;

        let mut instances: Vec<LabeledInstance<'_>> = inputs
            .into_par_iter()
            .map(|input| {
                let instance = ProofInstance::new(
                    config,
                    input.registry,
                    input.proving_metadata,
                    input.trace_map,
                )?;
                Ok(LabeledInstance {
                    tier: input.tier,
                    key: input.key,
                    instance,
                })
            })
            .collect::<Result<Vec<_>, ProveError>>()?;

        let commitments: Vec<MainCommitment> = instances
            .par_iter_mut()
            .map(|labeled| labeled.instance.commit_main())
            .collect::<Result<Vec<_>, ProveError>>()?;

        let mut transcript = MachineTranscript::new(config);
        transcript.observe_binding_digest(&binding_digest);
        for commitment in &commitments {
            transcript.observe_main_commitment(commitment);
        }

        let challenges = transcript.sample_logup_challenges();

        let summaries: Vec<BTreeMap<BusId, EF4>> = instances
            .par_iter_mut()
            .map(|labeled| labeled.instance.build_perm_traces(challenges))
            .collect::<Result<Vec<_>, ProveError>>()?;

        instances
            .par_iter()
            .zip(summaries.par_iter())
            .try_for_each(|(labeled, summary)| {
                Self::check_internal_balance(summary, labeled.tier, &external_buses)
            })?;

        let all_external: Vec<BTreeMap<BusId, EF4>> = summaries
            .iter()
            .map(|summary| extract_external_cumsums(summary, &external_buses))
            .collect();
        check_cross_proof_bus_balance(all_external.iter())
            .map_err(|(bus_id, total)| ProveError::CrossProofBusImbalance { bus_id, total })?;

        let challengers: Vec<Challenger> =
            (0..instances.len()).map(|_| transcript.fork()).collect();

        let all_results: Vec<_> = instances
            .into_par_iter()
            .zip(all_external.into_par_iter())
            .zip(challengers.into_par_iter())
            .map(|((labeled, exported), mut challenger)| {
                let sub = labeled.instance.prove(&mut challenger)?;
                Ok((labeled.tier, labeled.key, exported, sub))
            })
            .collect::<Result<Vec<_>, ProveError>>()?;

        let mut results = all_results.into_iter();

        let Some((_, _, exec_cumsums, exec_sub)) = results.next() else {
            return Err(ProveError::InvalidProofInput {
                detail: "missing execution proof result".to_string(),
            });
        };
        let exec_envelope = make_envelope(ProofTier::Execution, exec_sub, exec_cumsums);

        let column_entries: Vec<ColumnProofEntry> = results
            .by_ref()
            .take(num_cols)
            .map(|(tier, key, exported, sub)| {
                Ok(ColumnProofEntry {
                    proof: make_envelope(tier, sub, exported),
                    key: key.ok_or_else(|| ProveError::InvalidProofInput {
                        detail: format!("missing column key for {tier}"),
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
            columns: column_entries,
            root: root_envelope,
            binding_digest,
        })
    }

    fn assemble_instance_inputs<'b>(
        topology: &'b ProofTopology,
        traces: TierTraceBundle,
    ) -> Result<(Vec<InstanceInput<'b>>, usize), ProveError> {
        let TierTraceBundle {
            execution: execution_traces,
            columns: column_traces,
            root: root_traces,
        } = traces;

        if column_traces.len() != topology.columns.len() {
            return Err(ProveError::InvalidProofInput {
                detail: format!(
                    "column trace count {} does not match machine setup count {}",
                    column_traces.len(),
                    topology.columns.len()
                ),
            });
        }

        let num_columns = column_traces.len();
        let mut inputs = Vec::with_capacity(2 + num_columns);
        inputs.push(InstanceInput {
            tier: ProofTier::Execution,
            key: None,
            registry: &topology.execution.registry,
            proving_metadata: &topology.execution.proving_metadata,
            trace_map: execution_traces,
        });

        for (((table_id, col_id), tier_topology), column_trace) in
            topology.columns.iter().zip(column_traces.into_iter())
        {
            let key = column_trace.key;
            let expected = ColumnSlotKey {
                table: *table_id,
                col: *col_id,
            };
            if key != expected {
                return Err(ProveError::InvalidProofInput {
                    detail: format!(
                        "column trace order mismatch: trace bundle has {key} but setup expects {expected}",
                    ),
                });
            }

            inputs.push(InstanceInput {
                tier: ProofTier::Column { key: expected },
                key: Some(key),
                registry: &tier_topology.registry,
                proving_metadata: &tier_topology.proving_metadata,
                trace_map: column_trace.trace_map,
            });
        }

        inputs.push(InstanceInput {
            tier: ProofTier::Root,
            key: None,
            registry: &topology.root.registry,
            proving_metadata: &topology.root.proving_metadata,
            trace_map: root_traces,
        });

        Ok((inputs, num_columns))
    }

    fn check_internal_balance(
        summary: &BTreeMap<BusId, EF4>,
        tier: ProofTier,
        external_buses: &BTreeSet<BusId>,
    ) -> Result<(), ProveError> {
        for (&bus_id, &cumsum) in summary {
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
    key: Option<ColumnSlotKey>,
    instance: ProofInstance<'a>,
}

struct InstanceInput<'a> {
    tier: ProofTier,
    key: Option<ColumnSlotKey>,
    registry: &'a ChipRegistry,
    proving_metadata: &'a TierProvingMetadata,
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
    all: &BTreeMap<BusId, EF4>,
    external_buses: &BTreeSet<BusId>,
) -> BTreeMap<BusId, EF4> {
    all.iter()
        .filter(|(bus, _)| external_buses.contains(bus))
        .map(|(&bus, &cumsum)| (bus, cumsum))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use tabula_core::{ColId, TableId};
    use tabula_stark::trace::TraceMap;

    use super::Prover;
    use crate::TabulaMachine;
    use crate::backend::ProofColumn;
    use crate::input::ColumnSlotKey;
    use crate::input::assembly::{ColumnTraceBundle, TierTraceBundle};
    use crate::proof::errors::ProveError;
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

    fn column_trace(table_id: u32, col_id: u16) -> ColumnTraceBundle {
        ColumnTraceBundle {
            key: ColumnSlotKey {
                table: TableId(table_id),
                col: ColId(col_id),
            },
            trace_map: TraceMap::new(),
        }
    }

    #[test]
    fn assemble_instance_inputs_accepts_valid_ordered_column_traces() {
        let machine = test_machine();
        let traces = TierTraceBundle {
            execution: TraceMap::new(),
            columns: vec![column_trace(1, 0), column_trace(1, 1)],
            root: TraceMap::new(),
        };

        let (inputs, num_columns) =
            Prover::assemble_instance_inputs(machine.topology.proof_topology(), traces)
                .expect("ordered inputs");

        assert_eq!(num_columns, 2);
        assert_eq!(inputs.len(), 4);
    }

    #[test]
    fn assemble_instance_inputs_rejects_column_count_mismatch() {
        let machine = test_machine();
        let traces = TierTraceBundle {
            execution: TraceMap::new(),
            columns: vec![column_trace(1, 0)],
            root: TraceMap::new(),
        };

        let Err(err) = Prover::assemble_instance_inputs(machine.topology.proof_topology(), traces)
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
        let traces = TierTraceBundle {
            execution: TraceMap::new(),
            columns: vec![column_trace(1, 1), column_trace(1, 0)],
            root: TraceMap::new(),
        };

        let Err(err) = Prover::assemble_instance_inputs(machine.topology.proof_topology(), traces)
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
