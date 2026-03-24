//! Canonical runtime-owned proof journal reduction.

mod digest;
mod reduce;
mod state;
mod tx;
mod types;

pub(crate) use reduce::{build_proof_journal, convert_batch};
pub(crate) use types::{JournalInput, ProofColumnSlot, ProofJournal};

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use rayon::ThreadPoolBuilder;
    use tabula_artifact::{State, StateEntry};
    use tabula_core::InMemoryStaticTables;
    use tabula_core::{ColId, RowKey, TableId};
    use tabula_testing::exec::compiled_program_from_source;
    use tabula_testing::fixtures::batch::{multi_tx_batch, single_tx_batch};
    use tabula_testing::fixtures::cases::{liquid_shielded_bump_runtime_case, peek_runtime_case};
    use tabula_testing::fixtures::programs::transfer_balances_source;
    use tabula_testing::fixtures::state::{single_cell_u64, three_account_balances};
    use tabula_types::u64_portable;

    use super::digest::journal_digest;
    use super::{JournalInput, build_proof_journal, convert_batch};
    use crate::TabulaRuntime;
    use crate::error::RuntimeError;
    use crate::testing::fixtures::compiled_program_with_property_query;

    fn state_with_extra_surface_cell(mut state: State) -> State {
        state.cells.push(StateEntry {
            table: 99,
            row: 0,
            col: 0,
            value: Some(u64_portable(9)),
        });
        state
    }

    #[test]
    fn prepared_batch_journal_slot_order_matches_proof_plan() {
        let case = peek_runtime_case();
        let runtime = TabulaRuntime::builder(compiled_program_from_source(case.source))
            .build()
            .expect("build runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");
        let batch = convert_batch(&case.batch, runtime.type_runtimes()).expect("convert batch");
        let static_tables = InMemoryStaticTables::new();
        let prepared = build_proof_journal(JournalInput {
            resolved_program: runtime.proof_program(),
            state: &case.state,
            batch: &batch,
            execution_journal: executed.execution_journal(),
            static_tables: &static_tables,
        })
        .expect("prepared proof journal");

        let expected_keys: Vec<_> = runtime
            .proof_program()
            .proof_plan()
            .column_slots()
            .iter()
            .map(|slot| (slot.table, slot.col))
            .collect();
        let actual_keys: Vec<_> = prepared
            .columns
            .iter()
            .map(|slot| (slot.table, slot.col))
            .collect();
        assert_eq!(actual_keys, expected_keys);
        assert_eq!(
            prepared.precompile_calls_by_slot.len(),
            runtime
                .proof_program()
                .proof_plan()
                .precompile_slots()
                .len()
        );
    }

    #[test]
    fn prepared_batch_journal_rejects_state_outside_declared_surface() {
        let case = peek_runtime_case();
        let runtime = TabulaRuntime::builder(compiled_program_from_source(case.source))
            .build()
            .expect("build runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");
        let batch = convert_batch(&case.batch, runtime.type_runtimes()).expect("convert batch");
        let static_tables = InMemoryStaticTables::new();
        let invalid_state = state_with_extra_surface_cell(case.state.clone());

        let err = build_proof_journal(JournalInput {
            resolved_program: runtime.proof_program(),
            state: &invalid_state,
            batch: &batch,
            execution_journal: executed.execution_journal(),
            static_tables: &static_tables,
        })
        .expect_err("state outside proof surface must fail");

        match err {
            RuntimeError::ValidationFailed { detail } => {
                assert!(detail.contains("outside the declared program state surface"));
            }
            other => panic!("unexpected error: {other}"),
        }
    }

    #[test]
    fn prepared_batch_journal_ignores_failed_tx_diagnostics() {
        let runtime =
            TabulaRuntime::builder(compiled_program_from_source(transfer_balances_source()))
                .build()
                .expect("runtime");
        let state: State = three_account_balances(1000, 500, 200);
        let batch_file = multi_tx_batch(vec![
            (0, vec![u64_portable(0), u64_portable(1), u64_portable(300)]),
            (0, vec![u64_portable(0), u64_portable(2), u64_portable(800)]),
            (0, vec![u64_portable(1), u64_portable(2), u64_portable(100)]),
        ]);
        let executed = runtime
            .execute(&state, &batch_file)
            .expect("execution succeeds");
        let batch = convert_batch(&batch_file, runtime.type_runtimes()).expect("convert batch");
        let static_tables = InMemoryStaticTables::new();
        let prepared = build_proof_journal(JournalInput {
            resolved_program: runtime.proof_program(),
            state: &state,
            batch: &batch,
            execution_journal: executed.execution_journal(),
            static_tables: &static_tables,
        })
        .expect("prepared proof journal");

        let access_tx_indices: Vec<_> = prepared
            .columns
            .iter()
            .flat_map(|slot| slot.access_events.iter().map(|event| event.tx_index))
            .collect();
        assert_eq!(access_tx_indices, vec![0, 0, 0, 0, 2, 2, 2, 2]);
        assert!(
            prepared
                .columns
                .iter()
                .all(|slot| slot.property_reads.is_empty())
        );
        assert!(prepared.precompile_calls_by_slot.iter().all(Vec::is_empty));
        assert!(prepared.precompile_transcript_calls.is_empty());
        let lowering_tx_indices: Vec<_> = prepared
            .lowering
            .instruction_records
            .iter()
            .map(|record| record.tx_index)
            .collect();
        assert!(!lowering_tx_indices.is_empty());
        assert!(lowering_tx_indices.iter().all(|idx| matches!(*idx, 0 | 2)));
        assert!(lowering_tx_indices.contains(&0));
        assert!(lowering_tx_indices.contains(&2));
    }

    #[test]
    fn prepared_batch_journal_is_thread_count_deterministic() {
        let case = liquid_shielded_bump_runtime_case();
        let runtime = TabulaRuntime::builder(compiled_program_from_source(case.source))
            .build()
            .expect("build runtime");
        let executed = runtime
            .execute(&case.state, &case.batch)
            .expect("execution succeeds");
        let batch = convert_batch(&case.batch, runtime.type_runtimes()).expect("convert batch");
        let static_tables = InMemoryStaticTables::new();

        let single = ThreadPoolBuilder::new()
            .num_threads(1)
            .build()
            .expect("single-thread pool")
            .install(|| {
                build_proof_journal(JournalInput {
                    resolved_program: runtime.proof_program(),
                    state: &case.state,
                    batch: &batch,
                    execution_journal: executed.execution_journal(),
                    static_tables: &static_tables,
                })
                .expect("single-thread prepared proof journal")
            });
        let multi = ThreadPoolBuilder::new()
            .num_threads(4)
            .build()
            .expect("multi-thread pool")
            .install(|| {
                build_proof_journal(JournalInput {
                    resolved_program: runtime.proof_program(),
                    state: &case.state,
                    batch: &batch,
                    execution_journal: executed.execution_journal(),
                    static_tables: &static_tables,
                })
                .expect("multi-thread prepared proof journal")
            });

        assert_eq!(journal_digest(&single), journal_digest(&multi));
    }

    #[test]
    fn prepared_batch_journal_carries_property_reads_in_column_slots() {
        let runtime = TabulaRuntime::builder(compiled_program_with_property_query())
            .build()
            .expect("build runtime");
        let state: State = single_cell_u64(TableId(1), ColId(0), RowKey(5), 20);
        let batch_file = single_tx_batch(1, vec![]);
        let executed = runtime
            .execute(&state, &batch_file)
            .expect("execution succeeds");
        let batch = convert_batch(&batch_file, runtime.type_runtimes()).expect("convert batch");
        let static_tables = InMemoryStaticTables::new();
        let prepared = build_proof_journal(JournalInput {
            resolved_program: runtime.proof_program(),
            state: &state,
            batch: &batch,
            execution_journal: executed.execution_journal(),
            static_tables: &static_tables,
        })
        .expect("prepared proof journal");

        let property_counts: BTreeMap<_, _> = prepared
            .columns
            .iter()
            .filter(|slot| !slot.property_reads.is_empty())
            .map(|slot| ((slot.table, slot.col), slot.property_reads.len()))
            .collect();
        assert!(!property_counts.is_empty());
    }
}
