use std::cmp::Ordering;

use tabula_core::error::TabulaError;
use tabula_core::testing::{Blake3Hasher, InMemoryState};
use tabula_core::{CommittedCellKey, CommittedKey, CommittedPropertyQuery, TableId};
use tabula_executor as exec;
use tabula_ir as ir;
use tabula_lang::hir;
use tabula_profile::{TYPE_BYTES32_ID, TYPE_U64_ID};
use tabula_runtime::semantics::RuntimeProgram;
use tabula_types::{
    CommittedColumnEntry, NativeKeyPayload, TypeRuntimeRegistry, TypedCommittedPropertyQueryResult,
    TypedValue, encode_structural_u64, u64_typed,
};

use super::lower_hir_to_mir;
use crate::mir;

fn prelude() -> tabula_lang::FrontendPrelude {
    tabula_lang::FrontendPrelude::new(
        tabula_profile::builtin_semantic_registry().expect("registry"),
        vec![tabula_lang::CapabilityPreludeEntry {
            path: "poseidon_hash".into(),
            inputs: vec![TYPE_U64_ID],
            outputs: vec![TYPE_BYTES32_ID],
            totality: hir::CapabilityTotality::Total,
            query_policy: hir::CapabilityQueryPolicy::QuerySafe,
            proof_visibility: hir::CapabilityProofVisibility::OpaqueRuntimeOnly,
            hash_family: Some(hir::HashFamily::Poseidon),
        }],
    )
    .expect("prelude")
}

struct IrStateRuntime<'a> {
    program: &'a ir::Program,
}

impl<'a> IrStateRuntime<'a> {
    fn table(&self, table: ir::TableId) -> Result<&ir::TableSchema, TabulaError> {
        self.program
            .state
            .tables
            .iter()
            .find(|schema| schema.id == table)
            .ok_or_else(|| TabulaError::InvalidIr(format!("unknown state table {}", table.0)))
    }

    fn field(
        &self,
        table: ir::TableId,
        field: ir::FieldId,
    ) -> Result<&ir::FieldSchema, TabulaError> {
        self.table(table)?
            .fields
            .iter()
            .find(|schema| schema.id == field)
            .ok_or_else(|| {
                TabulaError::InvalidIr(format!("unknown state field {}.{}", table.0, field.0))
            })
    }
}

impl exec::StateRuntimeView for IrStateRuntime<'_> {
    fn encode_cell_key(
        &self,
        table: ir::TableId,
        field: ir::FieldId,
        key: &[TypedValue],
    ) -> Result<CommittedCellKey, TabulaError> {
        Ok(CommittedCellKey {
            table: TableId(table.0),
            col: field.into(),
            key: self.encode_committed_key(table, key)?,
        })
    }

    fn encode_committed_key(
        &self,
        table: ir::TableId,
        key: &[TypedValue],
    ) -> Result<CommittedKey, TabulaError> {
        let key_types = self.key_component_types(table)?;
        let [value] = key else {
            return Err(TabulaError::InvalidIr(
                "compiler test state runtime expects single-component keys".into(),
            ));
        };
        if key_types != vec![TYPE_U64_ID] || value.type_id() != TYPE_U64_ID {
            return Err(TabulaError::InvalidIr(format!(
                "compiler test state runtime only supports [u64] keys, table {} declared {:?}",
                table.0, key_types
            )));
        }
        Ok(CommittedKey(value.payload().to_vec()))
    }

    fn decode_committed_key(
        &self,
        table: ir::TableId,
        key: &CommittedKey,
    ) -> Result<Vec<TypedValue>, TabulaError> {
        let key_types = self.key_component_types(table)?;
        if key_types != vec![TYPE_U64_ID] || key.0.len() != std::mem::size_of::<u64>() {
            return Err(TabulaError::InvalidIr(format!(
                "compiler test state runtime only supports canonical [u64] keys for table {}",
                table.0
            )));
        }
        Ok(vec![u64_typed(u64::from_le_bytes(
            key.0.clone().try_into().expect("u64 key bytes"),
        ))])
    }

    fn encode_key_payload(
        &self,
        table: ir::TableId,
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
                phase: "compiler_hir_test_key_payload",
                detail: "failed to build fixed-width key payload".into(),
            })
    }

    fn compare_keys(
        &self,
        table: ir::TableId,
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

    fn key_component_types(
        &self,
        table: ir::TableId,
    ) -> Result<Vec<tabula_core::TypeId>, TabulaError> {
        Ok(self.table(table)?.keys.iter().map(|key| key.ty).collect())
    }

    fn column_type(
        &self,
        table: ir::TableId,
        field: ir::FieldId,
    ) -> Result<tabula_core::TypeId, TabulaError> {
        Ok(self.field(table, field)?.ty)
    }

    fn resolve_property(
        &self,
        _table: ir::TableId,
        _field: ir::FieldId,
        _query: &CommittedPropertyQuery,
        _state: &[CommittedColumnEntry],
    ) -> Result<TypedCommittedPropertyQueryResult, TabulaError> {
        Err(TabulaError::InvalidIr(
            "compiler HIR lowering tests do not use property reads".into(),
        ))
    }
}

#[test]
fn lowering_builds_verified_mir_from_hir() {
    let source = r#"
use capability poseidon_hash;

program Registry

state {
  table users(key id: u64) {
    active: bool @ssmc;
    tier: u64 @ssmc;
  }
}

const MAX_TIER: u64 = 3;

relation AllowedTier(tier: u64) = enum { 0, 1, 2, 3 };

fn validate_tier(tier: u64) {
  assert relation AllowedTier(tier);
  return;
}

tx register(id: u64, tier: u64) {
  validate_tier(tier);
  let digest = poseidon_hash(tier);
  assert select(true, true, true);
  users[id].active = true;
  users[id].tier = tier;
  return;
}
"#;

    let hir = tabula_lang::compile_to_hir(source, &prelude()).expect("hir");
    let mir = lower_hir_to_mir(&hir, ir::ProgramId(99)).expect("mir");
    assert_eq!(mir.program_id, ir::ProgramId(99));
    let verified = mir::verify_program(mir).expect("verified");
    let analyzed = mir::analyze_program(verified).expect("analyzed");
    let normalized = mir::inline_functions(&analyzed).expect("normalized");
    let canonicalized = mir::canonicalize_program(&normalized).expect("canonicalized");
    let analyzed = mir::analyze_program(canonicalized).expect("reanalyzed");
    let canonical = mir::lower_to_canonical(&analyzed).expect("canonical");
    let validated = ir::ValidatedProgram::try_from(canonical).expect("validated");
    let runtime = RuntimeProgram::from_validated_program(validated).expect("runtime");
    let state = InMemoryState::default();
    let runtimes = TypeRuntimeRegistry::seeded().expect("seeded runtimes");
    let state_runtime = IrStateRuntime {
        program: runtime.execution().program(),
    };
    let exec_ctx = exec::ExecContext {
        hasher: &Blake3Hasher,
        type_runtimes: &runtimes,
        capability_executor: None,
        state_runtime: &state_runtime,
    };
    let context = exec::ContextValues::default();
    let result = exec::execute_batch(
        runtime.execution(),
        &[exec::TxCall {
            entry_id: ir::EntryId(1),
            params: vec![u64_typed(1), u64_typed(2)],
        }],
        &context,
        &state,
        &exec_ctx,
    )
    .expect("execute");
    assert_eq!(result.txs.len(), 1);
    assert!(matches!(
        result.txs[0],
        exec::TxExecutionOutcome::Success(_)
    ));
}

#[test]
fn lowering_supports_v2_context_query_and_emit() {
    let source = r#"
program Registry

context {
  caller: u64;
}

event Registered(id: u64, actor: u64);

query current_actor(seed: u64) -> u64 {
  let actor = caller;
  return select(true, actor, seed);
}

tx register(id: u64) {
  emit Registered(id, caller);
  return;
}
"#;

    let hir = tabula_lang::compile_to_hir(source, &prelude()).expect("hir");
    let mir = lower_hir_to_mir(&hir, ir::ProgramId(7)).expect("mir");
    let query = mir
        .callables
        .iter()
        .find(|callable| callable.kind == mir::CallableKind::Query)
        .expect("query callable");
    assert!(
        query
            .body
            .region
            .ops
            .iter()
            .any(|op| matches!(op, mir::Op::BindValue { .. }))
    );

    let tx = mir
        .callables
        .iter()
        .find(|callable| callable.kind == mir::CallableKind::Tx)
        .expect("tx callable");
    assert!(
        tx.body
            .region
            .ops
            .iter()
            .any(|op| matches!(op, mir::Op::EmitEvent { .. }))
    );
}

#[test]
fn lowering_supports_v3_statement_level_if_and_match() {
    let source = r#"
program Control

tx choose(flag: bool, value: u64) {
  if flag {
    let selected = value;
  } else {
    let selected = 0;
  }
  match value {
    0 => {
      assert true;
    }
    _ => {
      assert true;
    }
  }
  return;
}
"#;

    let hir = tabula_lang::compile_to_hir(source, &prelude()).expect("hir");
    let mir = lower_hir_to_mir(&hir, ir::ProgramId(1)).expect("mir");
    let callable = mir
        .callables
        .iter()
        .find(|callable| callable.symbol == "choose")
        .unwrap();
    assert!(
        callable
            .body
            .region
            .ops
            .iter()
            .any(|op| matches!(op, mir::Op::If { .. }))
    );
    assert!(
        callable
            .body
            .region
            .ops
            .iter()
            .any(|op| matches!(op, mir::Op::Match { .. }))
    );
}
