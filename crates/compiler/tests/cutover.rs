use tabula_compiler::{
    CompilerCatalogs, compile_and_register_program_source, compile_program_source_with_catalogs,
};
use tabula_core::testing::{Blake3Hasher, InMemoryState};
use tabula_executor as exec;
use tabula_ir::{ContextInput, EntryBatch, EntryCall};
use tabula_runtime::{StateSnapshot, TabulaRuntime, semantics::RuntimeProgram};
use tabula_types::{TypeRuntimeRegistry, u64_portable, u64_typed};

#[test]
fn compiled_program_and_registered_runtime_have_matching_execution_outcomes() {
    let source = r#"
program Balances

state {
  table balances(key id: u64) {
    balance: u64 @ssmc;
  }
}

tx set_balance(id: u64, amount: u64) {
  balances[id].balance = amount;
  return;
}
"#;

    let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded runtimes");

    let compiled = compile_program_source_with_catalogs(source, &CompilerCatalogs::standard())
        .expect("compile");
    let runtime_program = RuntimeProgram::from_validated_program(compiled.into_validated_program())
        .expect("runtime program");
    let entry = runtime_program
        .execution()
        .program()
        .entries
        .iter()
        .find(|entry| entry.symbol == "set_balance")
        .expect("tx entry");
    let compiled_journal = exec::execute_batch(
        runtime_program.execution(),
        &[exec::TxCall {
            entry_id: entry.id,
            params: vec![u64_typed(5), u64_typed(7)],
        }],
        &exec::ContextValues::default(),
        &InMemoryState::default(),
        &exec::ExecContext {
            hasher: &Blake3Hasher,
            type_runtimes: &type_runtimes,
            capability_executor: None,
            property_reads: None,
        },
    )
    .expect("execute");

    let registered = compile_and_register_program_source(source, &CompilerCatalogs::standard())
        .expect("register");
    let runtime = TabulaRuntime::builder(registered).build().expect("runtime");
    let snapshot = StateSnapshot::empty(runtime.execution_program().program());
    let runtime_batch = EntryBatch {
        calls: vec![EntryCall {
            entry_id: entry.id,
            params: vec![u64_portable(5), u64_portable(7)],
        }],
    };
    let runtime_journal = runtime
        .execute_batch(&snapshot, &runtime_batch, &ContextInput::default())
        .expect("runtime execute");

    assert_eq!(compiled_journal.txs.len(), 1);
    assert_eq!(runtime_journal.txs.len(), 1);
    assert!(matches!(
        compiled_journal.txs[0],
        exec::TxExecutionOutcome::Success(_)
    ));
    assert!(matches!(
        runtime_journal.txs[0],
        exec::TxExecutionOutcome::Success(_)
    ));

    assert_eq!(compiled_journal.state_summary.write_set_final.len(), 1);
    assert_eq!(runtime_journal.state_summary.write_set_final.len(), 1);
    assert_eq!(
        compiled_journal.state_summary.write_set_final[0].value,
        Some(u64_typed(7))
    );
    assert_eq!(
        runtime_journal.state_summary.write_set_final[0].value,
        Some(u64_typed(7))
    );
    assert!(compiled_journal.state_summary.read_set_old.is_empty());
    assert!(runtime_journal.state_summary.read_set_old.is_empty());
}
