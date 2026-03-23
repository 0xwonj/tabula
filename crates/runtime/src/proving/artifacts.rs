//! Backend preparation from the canonical runtime proof journal.

use std::sync::Arc;

use rayon::prelude::*;
use tabula_chips::precompile_transcript::PRECOMPILE_TRANSCRIPT_WITNESS_LABEL;
use tabula_commitment::{PoseidonHasher, compute_state_roots_from_bindings};
use tabula_ext::backend::precompile::PrecompileProofContext;
use tabula_ext::backend::scheme::{ColumnProofContext, PreparedColumnDelta};
use tabula_machine::{ColumnIdentity, PublicStatement};
use tabula_stark::trace::WitnessStore;
use tabula_witness::stark::{SharedStoreBuilder, SharedStoreContext};

use crate::error::RuntimeError;
use crate::program::ResolvedProofProgram;

use super::journal::{ProofColumnSlot, ProofJournal};

pub(crate) struct ColumnTraceInput {
    pub(crate) identity: ColumnIdentity,
    pub(crate) store: WitnessStore,
}

/// Backend-prepared machine-facing proof bundle.
pub(crate) struct ProofArtifacts {
    pub(crate) air_statement: PublicStatement,
    pub(crate) shared_store: WitnessStore,
    pub(crate) columns: Vec<ColumnTraceInput>,
}

pub(crate) fn prepare_proof_artifacts(
    resolved_program: &ResolvedProofProgram,
    journal: ProofJournal,
) -> Result<ProofArtifacts, RuntimeError> {
    let column_slots = resolved_program.proof_plan().column_slots();
    let precompile_slots = resolved_program.proof_plan().precompile_slots();
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
        .zip(journal.columns.into_par_iter())
        .map(|(slot, column)| prepare_column_slot(slot, column))
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
    let hasher = PoseidonHasher::new();
    let (old_state_root, new_state_root) =
        compute_state_roots_from_bindings(&hasher, &root_bindings)
            .map_err(RuntimeError::TraceBuild)?;
    let air_statement = PublicStatement {
        old_root: old_state_root,
        new_root: new_state_root,
    };

    let mut shared_store = SharedStoreBuilder::<PoseidonHasher, 3>::new(SharedStoreContext {
        column_root_bindings: &root_bindings,
        old_state_root: &air_statement.old_root,
        new_state_root: &air_statement.new_root,
    })
    .prepare_witness_store(&journal.lowering, PoseidonHasher::new())
    .map_err(RuntimeError::TraceBuild)?;
    if !journal.precompile_transcript_calls.is_empty() {
        let mut transcript_store = WitnessStore::new();
        transcript_store.put(
            PRECOMPILE_TRANSCRIPT_WITNESS_LABEL,
            journal.precompile_transcript_calls,
        );
        shared_store
            .merge(transcript_store)
            .map_err(|detail| RuntimeError::WitnessGeneration { detail })?;
    }
    for prepared in prepared_precompiles {
        shared_store
            .merge(prepared.store)
            .map_err(|detail| RuntimeError::WitnessGeneration { detail })?;
    }

    let columns = prepared_columns
        .into_iter()
        .map(|(table, col, proof)| ColumnTraceInput {
            identity: ColumnIdentity {
                table_id: table.0,
                col_id: col.0,
                com_old: proof.old_digest.digest.0,
                com_new: proof.new_digest.digest.0,
            },
            store: proof.store,
        })
        .collect();

    Ok(ProofArtifacts {
        air_statement,
        shared_store,
        columns,
    })
}

fn prepare_column_slot(
    slot: &crate::program::ColumnProofSlot,
    column: ProofColumnSlot,
) -> Result<
    (
        tabula_core::TableId,
        tabula_core::ColId,
        tabula_ext::backend::scheme::PreparedColumnProof,
    ),
    RuntimeError,
> {
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
    use tabula_chips::shards::shared::{SHARED_COLUMN_WITNESS_LABEL, SharedColumnWitness};
    use tabula_chips::shards::smt_state::{SMT_STATE_WITNESS_LABEL, SmtStateWitness};
    use tabula_core::{ColId, InMemoryStaticTables};
    use tabula_testing::exec::compiled_program_from_source;
    use tabula_testing::fixtures::cases::{
        liquid_shielded_bump_runtime_case, peek_runtime_case, shielded_peek_runtime_case,
    };
    use tabula_testing::fixtures::state::empty_state;

    use crate::TabulaRuntime;
    use crate::proving::{JournalInput, build_proof_journal, convert_batch};

    use super::ProofArtifacts;
    use super::prepare_proof_artifacts;

    fn column_store(
        prepared: &ProofArtifacts,
        col_id: ColId,
    ) -> &tabula_stark::trace::WitnessStore {
        prepared
            .columns
            .iter()
            .find_map(|column| (column.identity.col_id == col_id.0).then_some(&column.store))
            .expect("column store")
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
        let batch = convert_batch(&case.batch, runtime.type_runtimes()).expect("convert batch");
        let static_tables = InMemoryStaticTables::new();
        let journal = build_proof_journal(JournalInput {
            resolved_program: runtime.proof_program(),
            state: &case.state,
            batch: &batch,
            execution_journal: executed.execution_journal(),
            static_tables: &static_tables,
        })
        .expect("prepared batch journal");
        let prepared =
            prepare_proof_artifacts(runtime.proof_program(), journal).expect("prepared artifacts");

        let actual: Vec<_> = prepared
            .columns
            .iter()
            .map(|column| (column.identity.table_id, column.identity.col_id))
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
        let batch = convert_batch(&case.batch, runtime.type_runtimes()).expect("convert batch");
        let static_tables = InMemoryStaticTables::new();
        let prepared = prepare_proof_artifacts(
            runtime.proof_program(),
            build_proof_journal(JournalInput {
                resolved_program: runtime.proof_program(),
                state: &case.state,
                batch: &batch,
                execution_journal: executed.execution_journal(),
                static_tables: &static_tables,
            })
            .expect("prepared batch journal"),
        )
        .expect("prepared proof artifacts");

        let identity = prepared.columns[0].identity;
        assert_eq!(identity.com_new, identity.com_old);

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
        let batch = convert_batch(&case.batch, runtime.type_runtimes()).expect("convert batch");
        let static_tables = InMemoryStaticTables::new();
        let prepared = prepare_proof_artifacts(
            runtime.proof_program(),
            build_proof_journal(JournalInput {
                resolved_program: runtime.proof_program(),
                state: &state,
                batch: &batch,
                execution_journal: executed.execution_journal(),
                static_tables: &static_tables,
            })
            .expect("prepared batch journal"),
        )
        .expect("prepared proof artifacts");

        let identity = prepared.columns[0].identity;
        assert_eq!(identity.com_new, identity.com_old);

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
        let batch = convert_batch(&case.batch, runtime.type_runtimes()).expect("convert batch");
        let static_tables = InMemoryStaticTables::new();
        let prepared = prepare_proof_artifacts(
            runtime.proof_program(),
            build_proof_journal(JournalInput {
                resolved_program: runtime.proof_program(),
                state: &case.state,
                batch: &batch,
                execution_journal: executed.execution_journal(),
                static_tables: &static_tables,
            })
            .expect("prepared batch journal"),
        )
        .expect("prepared proof artifacts");

        let identity = prepared.columns[0].identity;
        assert_eq!(identity.com_new, identity.com_old);

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
        let batch = convert_batch(&case.batch, runtime.type_runtimes()).expect("convert batch");
        let static_tables = InMemoryStaticTables::new();
        let prepared = prepare_proof_artifacts(
            runtime.proof_program(),
            build_proof_journal(JournalInput {
                resolved_program: runtime.proof_program(),
                state: &case.state,
                batch: &batch,
                execution_journal: executed.execution_journal(),
                static_tables: &static_tables,
            })
            .expect("prepared batch journal"),
        )
        .expect("prepared proof artifacts");

        let liquid = prepared
            .columns
            .iter()
            .map(|column| column.identity)
            .find(|identity| identity.col_id == ColId(0).0)
            .expect("liquid column");
        assert_eq!(liquid.com_new, liquid.com_old);

        let shielded = prepared
            .columns
            .iter()
            .map(|column| column.identity)
            .find(|identity| identity.col_id == ColId(1).0)
            .expect("shielded column");
        assert_ne!(shielded.com_new, shielded.com_old);
    }
}
