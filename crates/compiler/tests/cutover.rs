//! Tests that compare compiler and runtime execution behavior.

use std::cmp::Ordering;

use tabula_compiler::{
    CompilerCatalogs, compile_and_register_program_source, compile_program_source_with_catalogs,
};
use tabula_core::error::TabulaError;
use tabula_core::testing::{Blake3Hasher, InMemoryState};
use tabula_core::{ColId, CommittedCellKey, CommittedKey, CommittedPropertyQuery, TableId, TypeId};
use tabula_executor as exec;
use tabula_ir::{ContextInput, EntryBatch, EntryCall};
use tabula_profile::TYPE_U64_ID;
use std::sync::Arc;

use tabula_runtime::{PreparedOptions, prepare_executor, semantics::RuntimeProgram};
use tabula_types::{
    CommittedColumnEntry, ContextValues, NativeKeyPayload, StateRuntimeView, TxCall,
    TypeRuntimeRegistry, TypedCommittedPropertyQueryResult, TypedValue, encode_structural_u64,
    u64_portable, u64_typed,
};

#[derive(Default)]
struct TestStateRuntime;

impl StateRuntimeView for TestStateRuntime {
    fn encode_cell_key(
        &self,
        table: tabula_ir::TableId,
        field: tabula_ir::FieldId,
        key: &[TypedValue],
    ) -> Result<CommittedCellKey, TabulaError> {
        Ok(CommittedCellKey {
            table: TableId(table.0),
            col: ColId(field.0),
            key: self.encode_committed_key(table, key)?,
        })
    }

    fn encode_committed_key(
        &self,
        _table: tabula_ir::TableId,
        key: &[TypedValue],
    ) -> Result<CommittedKey, TabulaError> {
        let [value] = key else {
            return Err(TabulaError::InvalidIr(
                "test state runtime expects single-component keys".into(),
            ));
        };
        if value.type_id() != TYPE_U64_ID {
            return Err(TabulaError::InvalidIr(format!(
                "test state runtime expects u64 keys, got {}",
                value.type_id().0
            )));
        }
        Ok(CommittedKey(value.payload().to_vec()))
    }

    fn decode_committed_key(
        &self,
        _table: tabula_ir::TableId,
        key: &CommittedKey,
    ) -> Result<Vec<TypedValue>, TabulaError> {
        if key.0.len() != std::mem::size_of::<u64>() {
            return Err(TabulaError::InvalidIr(format!(
                "expected 8 committed-key bytes, got {}",
                key.0.len()
            )));
        }
        let raw = u64::from_le_bytes(key.0.clone().try_into().expect("u64 bytes"));
        Ok(vec![u64_typed(raw)])
    }

    fn encode_key_payload(
        &self,
        table: tabula_ir::TableId,
        key: &CommittedKey,
    ) -> Result<NativeKeyPayload, TabulaError> {
        let [value]: [TypedValue; 1] = self
            .decode_committed_key(table, key)?
            .try_into()
            .map_err(|_| TabulaError::InvalidIr("expected one key component".into()))?;
        let raw = u64::from_le_bytes(value.payload().try_into().expect("u64 payload"));
        encode_structural_u64::<{ tabula_types::NATIVE_KEY_PAYLOAD_WIDTH }>(raw)?
            .try_into()
            .map_err(|_| TabulaError::ProofError {
                phase: "compiler_test_key_payload",
                detail: "failed to build fixed-width key payload".into(),
            })
    }

    fn compare_keys(
        &self,
        table: tabula_ir::TableId,
        lhs: &CommittedKey,
        rhs: &CommittedKey,
    ) -> Result<Ordering, TabulaError> {
        let [lhs]: [TypedValue; 1] = self
            .decode_committed_key(table, lhs)?
            .try_into()
            .map_err(|_| TabulaError::InvalidIr("expected one lhs key component".into()))?;
        let [rhs]: [TypedValue; 1] = self
            .decode_committed_key(table, rhs)?
            .try_into()
            .map_err(|_| TabulaError::InvalidIr("expected one rhs key component".into()))?;
        let lhs = u64::from_le_bytes(lhs.payload().try_into().expect("u64 payload"));
        let rhs = u64::from_le_bytes(rhs.payload().try_into().expect("u64 payload"));
        Ok(lhs.cmp(&rhs))
    }

    fn key_component_types(&self, _table: tabula_ir::TableId) -> Result<Vec<TypeId>, TabulaError> {
        Ok(vec![TYPE_U64_ID])
    }

    fn column_type(
        &self,
        _table: tabula_ir::TableId,
        _field: tabula_ir::FieldId,
    ) -> Result<TypeId, TabulaError> {
        Ok(TYPE_U64_ID)
    }

    fn resolve_property(
        &self,
        _table: tabula_ir::TableId,
        _field: tabula_ir::FieldId,
        _query: &CommittedPropertyQuery,
        _state: &[CommittedColumnEntry],
    ) -> Result<TypedCommittedPropertyQueryResult, TabulaError> {
        Err(TabulaError::InvalidIr(
            "property reads are not used in this cutover test".into(),
        ))
    }
}

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

    let compiled = compile_program_source_with_catalogs(
        source,
        &CompilerCatalogs::standard().expect("standard catalogs"),
    )
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
        &[TxCall {
            entry_id: entry.id,
            params: vec![u64_typed(5), u64_typed(7)],
        }],
        &ContextValues::default(),
        &InMemoryState::default(),
        &exec::ExecContext {
            hasher: &Blake3Hasher,
            type_runtimes: &type_runtimes,
            capability_executor: None,
            state_runtime: &TestStateRuntime,
        },
    )
    .expect("execute");

    let registered = compile_and_register_program_source(
        source,
        &CompilerCatalogs::standard().expect("standard catalogs"),
    )
    .expect("register");
    let opts = PreparedOptions::try_standard().expect("standard prepared options");
    let runtime = prepare_executor(Arc::new(registered), &opts).expect("prepared executor");
    let snapshot = runtime.empty_state_snapshot();
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
