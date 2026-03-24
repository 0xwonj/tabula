//! Backend preparation from the canonical runtime proof journal.

use std::sync::Arc;

use rayon::prelude::*;
use tabula_chips::precompile_transcript::PRECOMPILE_TRANSCRIPT_WITNESS_LABEL;
use tabula_ext::backend::precompile::PrecompileProofContext;
use tabula_ext::backend::scheme::{ColumnProofContext, PreparedColumnDelta};
use tabula_ext::root::RootWitnessContext;
use tabula_machine::{
    ColumnSlotKey, PreparedColumnInput, PreparedMachineInput, PreparedTierInput, PublicStatement,
};
use tabula_stark::trace::WitnessStore;
use tabula_witness::stark::prepare_execution_store;

use crate::error::RuntimeError;
use crate::program::ResolvedProofProgram;

use super::batch_plan::BatchProofPlan;
use super::journal::{ProofColumnSlot, ProofJournal};

struct PreparedColumnArtifacts {
    input: PreparedColumnInput,
    #[cfg(test)]
    com_old: [p3_koala_bear::KoalaBear; 8],
    #[cfg(test)]
    com_new: [p3_koala_bear::KoalaBear; 8],
}

/// Backend-prepared machine-facing proof bundle.
pub(super) struct ProofArtifacts {
    air_statement: PublicStatement,
    execution: PreparedTierInput,
    columns: Vec<PreparedColumnArtifacts>,
    root: PreparedTierInput,
}

impl ProofArtifacts {
    pub(super) fn air_statement(&self) -> &PublicStatement {
        &self.air_statement
    }

    pub(super) fn into_prepared_machine_input(
        self,
        statement_digest: [u8; 32],
    ) -> PreparedMachineInput {
        PreparedMachineInput {
            execution: self.execution,
            columns: self
                .columns
                .into_iter()
                .map(|column| column.input)
                .collect(),
            root: self.root,
            statement: self.air_statement,
            statement_digest,
        }
    }
}

pub(super) fn prepare_proof_artifacts(
    resolved_program: &ResolvedProofProgram,
    batch_plan: &BatchProofPlan,
    journal: ProofJournal,
) -> Result<ProofArtifacts, RuntimeError> {
    let column_slots = resolved_program.proof_plan().column_slots();
    let precompile_slots = resolved_program.proof_plan().precompile_slots();
    if batch_plan.columns.len() != column_slots.len() {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "batch proof plan column count {} does not match proof plan count {}",
                batch_plan.columns.len(),
                column_slots.len(),
            ),
        });
    }
    if journal.columns.len() != column_slots.len() {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "prepared column slot count {} does not match proof plan count {}",
                journal.columns.len(),
                column_slots.len(),
            ),
        });
    }
    if journal.precompile_calls_by_slot.len() != precompile_slots.len() {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "prepared precompile slot count {} does not match proof plan count {}",
                journal.precompile_calls_by_slot.len(),
                precompile_slots.len(),
            ),
        });
    }
    let prepared_columns = column_slots
        .par_iter()
        .zip(batch_plan.columns.par_iter())
        .zip(journal.columns.into_par_iter())
        .map(|((slot, planned), column)| prepare_column_slot(slot, planned.key, column))
        .collect::<Result<Vec<_>, RuntimeError>>()?;
    let prepared_precompiles = precompile_slots
        .iter()
        .zip(journal.precompile_calls_by_slot)
        .map(|(slot, calls)| {
            let context = PrecompileProofContext {
                descriptor: slot.descriptor.clone(),
                calls,
                binding: resolved_program.binding().clone(),
            };
            slot.preparer
                .prepare_precompile(context)
                .map_err(RuntimeError::from_extension_proof)
                .map_err(|error| match error {
                    RuntimeError::WitnessGeneration { detail } => RuntimeError::WitnessGeneration {
                        detail: format!(
                            "precompile 0x{:04x} proof preparer '{}': {detail}",
                            slot.descriptor.precompile_id.0,
                            slot.preparer.name(),
                        ),
                    },
                    other => other,
                })
        })
        .collect::<Result<Vec<_>, RuntimeError>>()?;

    let root_bindings: Vec<_> = prepared_columns
        .iter()
        .filter_map(|(_, _, proof)| proof.root_binding.clone())
        .collect();
    let witness_preparer = batch_plan.root.backend.witness_preparer();
    let prepared_root = witness_preparer
        .prepare_root_witness(RootWitnessContext::new(&root_bindings))
        .map_err(RuntimeError::from_extension_proof)
        .map_err(|error| match error {
            RuntimeError::WitnessGeneration { detail } => RuntimeError::WitnessGeneration {
                detail: format!(
                    "root witness preparer '{}': {detail}",
                    witness_preparer.name(),
                ),
            },
            other => other,
        })?;
    let (air_statement, root_store) = prepared_root.into_parts();

    let mut execution_store =
        prepare_execution_store(&journal.lowering).map_err(RuntimeError::TraceBuild)?;
    if !journal.precompile_transcript_calls.is_empty() {
        let mut transcript_store = WitnessStore::new();
        transcript_store.put(
            PRECOMPILE_TRANSCRIPT_WITNESS_LABEL,
            journal.precompile_transcript_calls,
        );
        execution_store
            .merge(transcript_store)
            .map_err(|detail| RuntimeError::WitnessGeneration { detail })?;
    }
    for prepared in prepared_precompiles {
        execution_store
            .merge(prepared.store)
            .map_err(|detail| RuntimeError::WitnessGeneration { detail })?;
    }

    let columns = prepared_columns
        .into_iter()
        .map(|(table, col, proof)| PreparedColumnArtifacts {
            input: PreparedColumnInput {
                key: ColumnSlotKey { table, col },
                store: proof.store,
            },
            #[cfg(test)]
            com_old: proof.old_digest.digest.0,
            #[cfg(test)]
            com_new: proof.new_digest.digest.0,
        })
        .collect();

    Ok(ProofArtifacts {
        air_statement,
        execution: PreparedTierInput {
            store: execution_store,
        },
        columns,
        root: PreparedTierInput { store: root_store },
    })
}

fn prepare_column_slot(
    slot: &crate::program::ColumnProofSlot,
    planned_key: ColumnSlotKey,
    column: ProofColumnSlot,
) -> Result<
    (
        tabula_core::TableId,
        tabula_core::ColId,
        tabula_ext::backend::scheme::PreparedColumnProof,
    ),
    RuntimeError,
> {
    let expected_key = ColumnSlotKey {
        table: slot.table,
        col: slot.col,
    };
    if planned_key != expected_key {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "batch proof plan column {planned_key} does not match proof plan slot {expected_key}",
            ),
        });
    }
    if column.table != slot.table || column.col != slot.col {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "prepared column slot ({}, {}) does not match proof plan slot ({}, {})",
                column.table.0, column.col.0, slot.table.0, slot.col.0,
            ),
        });
    }
    let backend_name = slot.proof_backend.name().to_string();
    let proof_backend = Arc::clone(&slot.proof_backend);
    let context = ColumnProofContext {
        column: {
            let is_touched = !column.writes.is_empty();
            PreparedColumnDelta {
                table: column.table,
                col: column.col,
                init_cells: column.init_cells,
                access_events: column.access_events,
                writes: column.writes,
                is_touched,
            }
        },
        old_entries: column.old_entries,
        property_reads: column.property_reads,
    };
    let proof = proof_backend
        .prepare_column(context)
        .map_err(RuntimeError::from_extension_proof)
        .map_err(|error| match error {
            RuntimeError::WitnessGeneration { detail } => RuntimeError::WitnessGeneration {
                detail: format!(
                    "column ({}, {}) proof backend '{}': {detail}",
                    slot.table.0, slot.col.0, backend_name,
                ),
            },
            other => other,
        })?;
    Ok((slot.table, slot.col, proof))
}

#[cfg(test)]
mod tests {
    use p3_field::PrimeCharacteristicRing;
    use p3_koala_bear::KoalaBear;
    use tabula_chips::precompile_transcript::{
        PRECOMPILE_TRANSCRIPT_WITNESS_LABEL, PrecompileTranscriptCall,
    };
    use tabula_chips::shards::shared::{SHARED_COLUMN_WITNESS_LABEL, SharedColumnWitness};
    use tabula_chips::shards::smt_state::{SMT_STATE_WITNESS_LABEL, SmtStateWitness};
    use tabula_chips::smt_path::trace::{SmtPathWitness, SmtTablePathWitness};
    use tabula_core::{ColId, InMemoryStaticTables};
    use tabula_ext::PrecompileBackendFactoryBundle;
    use tabula_ext::root::{
        PreparedRootWitness, RootBackend, RootBackendBundle, RootWitnessContext,
        RootWitnessPreparer,
    };
    use tabula_machine::PublicStatement;
    use tabula_stark::trace::{WitnessStore, witness_labels};
    use tabula_testing::exec::compiled_program_from_source;
    use tabula_testing::extensions::precompile::{
        CONSTANT_ONE_PRECOMPILE_ID, ConstantOnePrecompileBackendFactory,
        constant_one_precompile_descriptor,
    };
    use tabula_testing::fixtures::cases::{
        liquid_shielded_bump_runtime_case, peek_runtime_case, shielded_peek_runtime_case,
    };
    use tabula_testing::fixtures::compiled::compiled_precompile_requirement_case;
    use tabula_testing::fixtures::state::empty_state;

    use crate::TabulaRuntime;
    use crate::host::HostEnvironment;
    use crate::proving::{
        JournalInput, build_batch_proof_plan, build_proof_journal, convert_batch,
    };

    use super::{ProofArtifacts, prepare_proof_artifacts};

    const CUSTOM_ROOT_MARKER_LABEL: &str = "testing_custom_root_marker";

    #[derive(Clone, Copy, Debug)]
    struct DelegatingRootProofBackend;

    impl tabula_machine::RootProofBackend for DelegatingRootProofBackend {
        fn name(&self) -> &str {
            "delegating_root_proof"
        }

        fn supported_root_binding_families(&self) -> &'static [tabula_core::RootProfileId] {
            tabula_machine::SmtRootProofBackend.supported_root_binding_families()
        }

        fn airs(&self) -> Vec<Box<dyn tabula_machine::backend::AnyRap>> {
            tabula_machine::SmtRootProofBackend.airs()
        }

        fn dyn_chips(&self) -> Vec<Box<dyn tabula_stark::trace::DynChip>> {
            tabula_machine::SmtRootProofBackend.dyn_chips()
        }
    }

    #[derive(Debug)]
    struct CustomRootWitnessPreparer;

    impl RootWitnessPreparer for CustomRootWitnessPreparer {
        fn name(&self) -> &str {
            "custom_root"
        }

        fn prepare_root_witness(
            &self,
            _context: RootWitnessContext<'_>,
        ) -> Result<PreparedRootWitness, tabula_ext::ExtError> {
            let mut store = WitnessStore::new();
            store.put(CUSTOM_ROOT_MARKER_LABEL, vec![42u8]);
            Ok(PreparedRootWitness::new(
                PublicStatement {
                    old_root: tabula_commitment::NativeDigest([KoalaBear::ONE; 8]),
                    new_root: tabula_commitment::NativeDigest([KoalaBear::TWO; 8]),
                },
                store,
            ))
        }
    }

    #[derive(Clone, Copy, Debug)]
    struct CustomRootBackend;

    impl RootBackend for CustomRootBackend {
        fn name(&self) -> &str {
            "custom_root_backend"
        }

        fn proof_backend(&self) -> std::sync::Arc<dyn tabula_machine::RootProofBackend> {
            std::sync::Arc::new(DelegatingRootProofBackend)
        }

        fn witness_preparer(&self) -> std::sync::Arc<dyn RootWitnessPreparer> {
            std::sync::Arc::new(CustomRootWitnessPreparer)
        }
    }

    fn column_store(
        prepared: &ProofArtifacts,
        col_id: ColId,
    ) -> &tabula_stark::trace::WitnessStore {
        prepared
            .columns
            .iter()
            .find_map(|column| (column.input.key.col == col_id).then_some(&column.input.store))
            .expect("column store")
    }

    fn prepare_case_artifacts(
        runtime: &TabulaRuntime,
        state: &tabula_artifact::State,
        batch_file: &tabula_artifact::TransactionBatch,
        execution_journal: &tabula_executor::ExecutionJournal,
    ) -> ProofArtifacts {
        let batch = convert_batch(batch_file, runtime.type_runtimes()).expect("convert batch");
        let static_tables = InMemoryStaticTables::new();
        let journal = build_proof_journal(JournalInput {
            resolved_program: runtime.proof_program(),
            state,
            batch: &batch,
            execution_journal,
            static_tables: &static_tables,
        })
        .expect("prepared batch journal");
        let batch_plan =
            build_batch_proof_plan(runtime.proof_program(), runtime.root_backend_bundle())
                .expect("batch proof plan");
        prepare_proof_artifacts(runtime.proof_program(), &batch_plan, journal)
            .expect("prepared artifacts")
    }

    #[test]
    fn prepared_proof_artifacts_keep_column_slot_order() {
        let case = peek_runtime_case();
        let runtime = TabulaRuntime::builder(compiled_program_from_source(case.source))
            .build()
            .expect("build runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");
        let prepared = prepare_case_artifacts(
            &runtime,
            &case.state,
            &case.batch,
            executed.execution_journal(),
        );

        let actual: Vec<_> = prepared
            .columns
            .iter()
            .map(|column| (column.input.key.table.0, column.input.key.col.0))
            .collect();
        let expected: Vec<_> = runtime
            .proof_program()
            .proof_plan()
            .column_slots()
            .iter()
            .map(|slot| (slot.table.0, slot.col.0))
            .collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn ssmc_read_only_column_keeps_commitment_and_untouched_meta() {
        let case = peek_runtime_case();
        let runtime = TabulaRuntime::builder(compiled_program_from_source(case.source))
            .build()
            .expect("build runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");
        let prepared = prepare_case_artifacts(
            &runtime,
            &case.state,
            &case.batch,
            executed.execution_journal(),
        );

        let column = &prepared.columns[0];
        assert_eq!(column.com_new, column.com_old);

        let shared = column_store(&prepared, ColId(0))
            .get::<SharedColumnWitness>(SHARED_COLUMN_WITNESS_LABEL)
            .expect("shared witness");
        let meta_row = shared.meta_row.as_ref().expect("meta row");
        assert!(!meta_row.is_touched);
        assert_eq!(meta_row.empty_read_count, 0);
    }

    #[test]
    fn empty_read_only_ssmc_column_preserves_empty_state_and_records_empty_read() {
        let case = peek_runtime_case();
        let runtime = TabulaRuntime::builder(compiled_program_from_source(case.source))
            .build()
            .expect("build runtime");
        let state = empty_state();
        let executed = runtime
            .execute(&state, &case.batch)
            .expect("execution succeeds");
        let prepared =
            prepare_case_artifacts(&runtime, &state, &case.batch, executed.execution_journal());

        let column = &prepared.columns[0];
        assert_eq!(column.com_new, column.com_old);

        let shared = column_store(&prepared, ColId(0))
            .get::<SharedColumnWitness>(SHARED_COLUMN_WITNESS_LABEL)
            .expect("shared witness");
        let meta_row = shared.meta_row.as_ref().expect("meta row");
        assert!(!meta_row.is_touched);
        assert_eq!(meta_row.empty_read_count, 1);
    }

    #[test]
    fn smt_read_only_column_uses_trivial_no_write_semantics() {
        let case = shielded_peek_runtime_case();
        let runtime = TabulaRuntime::builder(compiled_program_from_source(case.source))
            .build()
            .expect("build runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");
        let prepared = prepare_case_artifacts(
            &runtime,
            &case.state,
            &case.batch,
            executed.execution_journal(),
        );

        let column = &prepared.columns[0];
        assert_eq!(column.com_new, column.com_old);

        let witness = column_store(&prepared, ColId(0))
            .get::<SmtStateWitness<3>>(SMT_STATE_WITNESS_LABEL)
            .expect("smt state witness");
        assert!(!witness.column_is_touched);
        assert_eq!(witness.column_new_root, witness.column_old_root);
        assert_eq!(witness.paths.len(), 1);
    }

    #[test]
    fn mixed_read_only_and_write_columns_preserve_per_column_touched_semantics() {
        let case = liquid_shielded_bump_runtime_case();
        let runtime = TabulaRuntime::builder(compiled_program_from_source(case.source))
            .build()
            .expect("build runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");
        let prepared = prepare_case_artifacts(
            &runtime,
            &case.state,
            &case.batch,
            executed.execution_journal(),
        );

        let liquid = prepared
            .columns
            .iter()
            .find(|column| column.input.key.col == ColId(0))
            .expect("liquid column");
        assert_eq!(liquid.com_new, liquid.com_old);

        let shielded = prepared
            .columns
            .iter()
            .find(|column| column.input.key.col == ColId(1))
            .expect("shielded column");
        assert_ne!(shielded.com_new, shielded.com_old);
    }

    #[test]
    fn prepare_proof_artifacts_routes_precompile_transcript_to_execution_tier_only() {
        let case = compiled_precompile_requirement_case();
        let host_environment = HostEnvironment::standard()
            .with_precompile_backend_bundle(PrecompileBackendFactoryBundle::new(
                ConstantOnePrecompileBackendFactory::new(constant_one_precompile_descriptor(
                    CONSTANT_ONE_PRECOMPILE_ID,
                )),
            ))
            .expect("register testing precompile backend");
        let runtime = TabulaRuntime::builder(case.compiled_program)
            .with_host_environment(host_environment)
            .build()
            .expect("build runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");
        let prepared = prepare_case_artifacts(
            &runtime,
            &case.state,
            &case.batch,
            executed.execution_journal(),
        );

        assert!(
            prepared
                .execution
                .store
                .contains::<Vec<PrecompileTranscriptCall>>(PRECOMPILE_TRANSCRIPT_WITNESS_LABEL)
        );
        assert!(
            !prepared
                .root
                .store
                .contains::<Vec<PrecompileTranscriptCall>>(PRECOMPILE_TRANSCRIPT_WITNESS_LABEL)
        );
    }

    #[test]
    fn prepare_proof_artifacts_routes_smt_root_witness_to_root_tier_only() {
        let case = peek_runtime_case();
        let runtime = TabulaRuntime::builder(compiled_program_from_source(case.source))
            .build()
            .expect("build runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");
        let prepared = prepare_case_artifacts(
            &runtime,
            &case.state,
            &case.batch,
            executed.execution_journal(),
        );

        assert!(
            prepared
                .execution
                .store
                .contains::<Vec<tabula_chips::execution::trace::InstructionRecord>>(
                    witness_labels::EXECUTION_RECORDS,
                )
        );
        assert!(
            prepared
                .root
                .store
                .contains::<Vec<SmtPathWitness>>(witness_labels::SMT_COL_PATHS)
        );
        assert!(
            prepared
                .root
                .store
                .contains::<Vec<SmtTablePathWitness>>(witness_labels::SMT_TABLE_PATHS)
        );
        assert!(
            prepared
                .root
                .store
                .contains::<Vec<KoalaBear>>(witness_labels::SMT_TABLE_PVS)
        );
        assert!(
            !prepared
                .execution
                .store
                .contains::<Vec<SmtPathWitness>>(witness_labels::SMT_COL_PATHS)
        );
        assert!(
            !prepared
                .execution
                .store
                .contains::<Vec<SmtTablePathWitness>>(witness_labels::SMT_TABLE_PATHS)
        );
        assert!(
            !prepared
                .execution
                .store
                .contains::<Vec<KoalaBear>>(witness_labels::SMT_TABLE_PVS)
        );
    }

    #[test]
    fn prepare_proof_artifacts_uses_root_witness_preparer_output() {
        let case = peek_runtime_case();
        let runtime = TabulaRuntime::builder(compiled_program_from_source(case.source))
            .with_root_backend_bundle(RootBackendBundle::new(CustomRootBackend))
            .build()
            .expect("build runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");
        let prepared = prepare_case_artifacts(
            &runtime,
            &case.state,
            &case.batch,
            executed.execution_journal(),
        );

        assert_eq!(
            prepared.air_statement.old_root,
            tabula_commitment::NativeDigest([KoalaBear::ONE; 8])
        );
        assert_eq!(
            prepared.air_statement.new_root,
            tabula_commitment::NativeDigest([KoalaBear::TWO; 8])
        );
        assert!(
            prepared
                .root
                .store
                .contains::<Vec<u8>>(CUSTOM_ROOT_MARKER_LABEL)
        );
        assert!(
            !prepared
                .root
                .store
                .contains::<Vec<SmtPathWitness>>(witness_labels::SMT_COL_PATHS)
        );
        assert!(
            !prepared
                .root
                .store
                .contains::<Vec<SmtTablePathWitness>>(witness_labels::SMT_TABLE_PATHS)
        );
        assert!(
            !prepared
                .root
                .store
                .contains::<Vec<KoalaBear>>(witness_labels::SMT_TABLE_PVS)
        );
    }
}
