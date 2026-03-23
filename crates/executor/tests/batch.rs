//! Batch executor integration tests.

mod common;

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::traits::Hasher;
use tabula_core::{
    AccessEvent, Batch, BatchReport, CellKey, ColId, EmittedEvent, OpKind, PrecompileEvent,
    PropertyReadResult, RowKey, TableId, TxResult, TxTypeId, TypeId,
};
use tabula_executor::ResolvedExecutionProgram;
use tabula_executor::consistency::check_journal_consistency;
use tabula_executor::precompile::PrecompileRegistry;
use tabula_executor::property::{
    CommittedStateProvider, PropertyQueryHandler, PropertyQueryRegistry,
};
use tabula_executor::{ExecutionJournal, TxExecutionOutcome, derive_batch_report, execute_batch};
use tabula_ir::{
    ArithOp, CmpOp, Instruction, ParamDef, PropertyQuery, RowExpr, TxTypeDef, ValueExpr,
};
use tabula_profile::TYPE_U64_ID;
use tabula_testing::assertions::{ExpectedTxOutcome, assert_tx_outcomes, assert_write_set_cell};
use tabula_testing::extensions::precompile::ConstantOnePrecompileHandler;
use tabula_testing::fixtures::compiled::compiled_precompile_requirement_program;
use tabula_types::{TypedColumnEntry, TypedPropertyQueryResult, u64_typed};

use common::*;

// ── Helpers ─────────────────────────────────────────────────────────────

fn test_program() -> tabula_ir::Program {
    let (schemas, profile_catalog) = test_schema_bundle();
    let mut program = tabula_ir::Program::with_profile_catalog(profile_catalog);
    program.add_schema(schemas.get(&TableId(1)).expect("test schema").clone());
    program
}

fn param(name: &str, type_id: TypeId) -> ParamDef {
    ParamDef {
        name: name.into(),
        type_id,
    }
}

fn write_tx_def() -> TxTypeDef {
    TxTypeDef {
        id: TxTypeId(1),
        name: "write_cell".into(),
        param_schema: vec![param("row", TYPE_U64_ID), param("value", TYPE_U64_ID)],
        body: vec![Instruction::Write {
            table: TableId(1),
            row: RowExpr::Param(0),
            col: ColId(0),
            src_val: ValueExpr::Param(1),
            src_is_null: lit(bool_portable(false)),
        }],
    }
}

fn transfer_tx_def() -> TxTypeDef {
    TxTypeDef {
        id: TxTypeId(2),
        name: "transfer".into(),
        param_schema: vec![param("amount", TYPE_U64_ID)],
        body: vec![
            Instruction::Read {
                dst_val: 0,
                dst_is_null: 1,
                table: TableId(1),
                row: RowExpr::Literal(RowKey(0)),
                col: ColId(0),
            },
            Instruction::Read {
                dst_val: 2,
                dst_is_null: 3,
                table: TableId(1),
                row: RowExpr::Literal(RowKey(1)),
                col: ColId(0),
            },
            Instruction::Cmp {
                dst: 4,
                op: CmpOp::Gte,
                lhs: ValueExpr::Slot(0),
                rhs: ValueExpr::Param(0),
            },
            Instruction::Assert {
                cond: ValueExpr::Slot(4),
            },
            Instruction::Arith {
                dst: 5,
                op: ArithOp::Sub,
                lhs: ValueExpr::Slot(0),
                rhs: ValueExpr::Param(0),
            },
            Instruction::Arith {
                dst: 6,
                op: ArithOp::Add,
                lhs: ValueExpr::Slot(2),
                rhs: ValueExpr::Param(0),
            },
            Instruction::Write {
                table: TableId(1),
                row: RowExpr::Literal(RowKey(0)),
                col: ColId(0),
                src_val: ValueExpr::Slot(5),
                src_is_null: lit(bool_portable(false)),
            },
            Instruction::Write {
                table: TableId(1),
                row: RowExpr::Literal(RowKey(1)),
                col: ColId(0),
                src_val: ValueExpr::Slot(6),
                src_is_null: lit(bool_portable(false)),
            },
        ],
    }
}

fn hash_tx_def() -> TxTypeDef {
    TxTypeDef {
        id: TxTypeId(4),
        name: "hash_only".into(),
        param_schema: vec![],
        body: vec![Instruction::Hash {
            dst: 0,
            inputs: vec![lit(u64_portable(7))],
        }],
    }
}

fn property_read_tx_def() -> TxTypeDef {
    TxTypeDef {
        id: TxTypeId(5),
        name: "property_read".into(),
        param_schema: vec![],
        body: vec![Instruction::PropertyRead {
            dst_val: 0,
            dst_key: 1,
            dst_is_null: 2,
            table: TableId(1),
            col: ColId(0),
            query: PropertyQuery::Minimum,
        }],
    }
}

fn execute_journal(
    batch: &Batch,
    program: &tabula_ir::Program,
    snapshot: &impl tabula_core::traits::StateView,
    env: &tabula_executor::batch::BatchEnv<'_>,
    initial_nonces: &BTreeMap<[u8; 32], u64>,
) -> ExecutionJournal {
    let resolved = ResolvedExecutionProgram::from_program(program).expect("resolved program");
    execute_batch(batch, &resolved, snapshot, env, initial_nonces).expect("execute batch")
}

fn execute_report(
    batch: &Batch,
    program: &tabula_ir::Program,
    snapshot: &impl tabula_core::traits::StateView,
    env: &tabula_executor::batch::BatchEnv<'_>,
    initial_nonces: &BTreeMap<[u8; 32], u64>,
) -> BatchReport {
    let journal = execute_journal(batch, program, snapshot, env, initial_nonces);
    derive_batch_report(&journal, env.type_runtimes).expect("derive batch result")
}

fn access_event(
    key: CellKey,
    op: OpKind,
    value: tabula_core::PortableValue,
    time: u64,
    effect_ordinal_in_tx: u32,
) -> AccessEvent {
    AccessEvent {
        key,
        op,
        value: portable(value),
        val_is_null: false,
        time,
        effect_ordinal_in_tx,
    }
}

#[derive(Default)]
struct MockCommittedState {
    columns: BTreeMap<(TableId, ColId), Vec<TypedColumnEntry>>,
}

impl CommittedStateProvider for MockCommittedState {
    fn get_column(&self, table: TableId, col: ColId) -> Result<Vec<TypedColumnEntry>, TabulaError> {
        self.columns
            .get(&(table, col))
            .cloned()
            .ok_or(TabulaError::TableNotFound(table))
    }
}

struct MinimumResolver;

impl PropertyQueryHandler for MinimumResolver {
    fn resolve(
        &self,
        query: &PropertyQuery,
        provider: &dyn CommittedStateProvider,
    ) -> Result<TypedPropertyQueryResult, TabulaError> {
        let entries = provider.get_column(TableId(1), ColId(0))?;
        match query {
            PropertyQuery::Minimum => {
                if let Some(entry) = entries
                    .iter()
                    .filter(|entry| !entry.is_null)
                    .min_by_key(|entry| entry.row_key.0)
                {
                    Ok(TypedPropertyQueryResult {
                        value: entry.value.clone(),
                        key: Some(entry.row_key),
                        is_null: false,
                    })
                } else {
                    Ok(TypedPropertyQueryResult {
                        value: u64_typed(0),
                        key: None,
                        is_null: true,
                    })
                }
            }
            other => Err(TabulaError::InvalidIr(format!(
                "unsupported property query in parity test: {other:?}"
            ))),
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[test]
fn single_successful_tx() {
    let snap = TestSnapshot(BTreeMap::new());
    let sender = [1u8; 32];
    let batch = Batch {
        transactions: vec![make_tx(
            1,
            vec![u64_portable(0), u64_portable(42)],
            sender,
            0,
        )],
    };
    let mut prog = test_program();
    prog.register(write_tx_def()).unwrap();

    let env = test_env();
    let result = execute_report(&batch, &prog, &snap, &env, &BTreeMap::new());
    assert_eq!(result.txs.len(), 1);
    assert_tx_outcomes(&result, &[ExpectedTxOutcome::Success]);
    assert_write_set_cell(
        &result,
        TableId(1),
        ColId(0),
        RowKey(0),
        Some(&u64_portable(42)),
    );
}

#[test]
fn simple_write_projection_matches_journal_shape() {
    let snap = TestSnapshot(BTreeMap::new());
    let sender = [1u8; 32];
    let batch = Batch {
        transactions: vec![make_tx(
            1,
            vec![u64_portable(0), u64_portable(42)],
            sender,
            0,
        )],
    };
    let mut prog = test_program();
    prog.register(write_tx_def()).unwrap();

    let env = test_env();
    let journal = execute_journal(&batch, &prog, &snap, &env, &BTreeMap::new());
    let result = derive_batch_report(&journal, env.type_runtimes).expect("derive batch result");
    assert_eq!(
        journal.state_summary.read_set_old,
        Vec::new(),
        "simple write should not read committed state",
    );
    assert_eq!(
        result,
        BatchReport {
            read_set_old: vec![],
            write_set_final: vec![(cell(1, 0, 0), Some(u64_portable(42)))],
            txs: vec![TxResult::Success {
                emitted: vec![],
                access_trace: vec![access_event(
                    cell(1, 0, 0),
                    OpKind::Write,
                    u64_portable(42),
                    0,
                    0
                )],
                precompile_events: vec![],
                property_reads: vec![],
            }],
        }
    );
}

#[test]
fn inter_tx_read_your_writes() {
    let snap = TestSnapshot(BTreeMap::new());
    let sender = [1u8; 32];
    let batch = Batch {
        transactions: vec![
            make_tx(1, vec![u64_portable(0), u64_portable(100)], sender, 0),
            make_tx(3, vec![], sender, 1),
        ],
    };
    let mut prog = test_program();
    prog.register(write_tx_def()).unwrap();
    prog.register(TxTypeDef {
        id: TxTypeId(3),
        name: "copy_cell".into(),
        param_schema: vec![],
        body: vec![
            Instruction::Read {
                dst_val: 0,
                dst_is_null: 1,
                table: TableId(1),
                row: RowExpr::Literal(RowKey(0)),
                col: ColId(0),
            },
            Instruction::Write {
                table: TableId(1),
                row: RowExpr::Literal(RowKey(1)),
                col: ColId(0),
                src_val: ValueExpr::Slot(0),
                src_is_null: lit(bool_portable(false)),
            },
        ],
    })
    .unwrap();

    let env = test_env();
    let result = execute_report(&batch, &prog, &snap, &env, &BTreeMap::new());
    assert_eq!(result.txs.len(), 2);
    assert_tx_outcomes(
        &result,
        &[ExpectedTxOutcome::Success, ExpectedTxOutcome::Success],
    );
    assert_write_set_cell(
        &result,
        TableId(1),
        ColId(0),
        RowKey(1),
        Some(&u64_portable(100)),
    );
}

#[test]
fn failed_tx_rollback() {
    let mut data = BTreeMap::new();
    data.insert(cell(1, 0, 0), u64_portable(50));
    data.insert(cell(1, 1, 0), u64_portable(50));
    let snap = snapshot(data);
    let sender = [1u8; 32];

    let batch = Batch {
        transactions: vec![
            make_tx(2, vec![u64_portable(30)], sender, 0),
            make_tx(2, vec![u64_portable(100)], sender, 1),
            make_tx(2, vec![u64_portable(10)], sender, 1),
        ],
    };
    let mut prog = test_program();
    prog.register(transfer_tx_def()).unwrap();

    let env = test_env();
    let result = execute_report(&batch, &prog, &snap, &env, &BTreeMap::new());
    assert_tx_outcomes(
        &result,
        &[
            ExpectedTxOutcome::Success,
            ExpectedTxOutcome::Failed,
            ExpectedTxOutcome::Success,
        ],
    );
    assert_write_set_cell(
        &result,
        TableId(1),
        ColId(0),
        RowKey(0),
        Some(&u64_portable(10)),
    );
    assert_write_set_cell(
        &result,
        TableId(1),
        ColId(0),
        RowKey(1),
        Some(&u64_portable(90)),
    );
}

#[test]
fn failed_tx_rollback_projection_matches_journal_shape() {
    let mut data = BTreeMap::new();
    data.insert(cell(1, 0, 0), u64_portable(50));
    data.insert(cell(1, 1, 0), u64_portable(50));
    let snap = snapshot(data);
    let sender = [1u8; 32];
    let batch = Batch {
        transactions: vec![
            make_tx(2, vec![u64_portable(30)], sender, 0),
            make_tx(2, vec![u64_portable(100)], sender, 1),
            make_tx(2, vec![u64_portable(10)], sender, 1),
        ],
    };
    let mut prog = test_program();
    prog.register(transfer_tx_def()).unwrap();

    let env = test_env();
    let journal = execute_journal(&batch, &prog, &snap, &env, &BTreeMap::new());
    let result = derive_batch_report(&journal, env.type_runtimes).expect("derive batch result");
    assert_eq!(
        result,
        BatchReport {
            read_set_old: vec![
                (cell(1, 0, 0), Some(u64_portable(50))),
                (cell(1, 1, 0), Some(u64_portable(50))),
            ],
            write_set_final: vec![
                (cell(1, 0, 0), Some(u64_portable(10))),
                (cell(1, 1, 0), Some(u64_portable(90))),
            ],
            txs: vec![
                TxResult::Success {
                    emitted: vec![],
                    access_trace: vec![
                        access_event(cell(1, 0, 0), OpKind::Read, u64_portable(50), 0, 0),
                        access_event(cell(1, 1, 0), OpKind::Read, u64_portable(50), 1, 1),
                        access_event(cell(1, 0, 0), OpKind::Write, u64_portable(20), 2, 2),
                        access_event(cell(1, 1, 0), OpKind::Write, u64_portable(80), 3, 3),
                    ],
                    precompile_events: vec![],
                    property_reads: vec![],
                },
                TxResult::Failed {
                    reason: "assertion failed: Slot(4)".to_string(),
                    partial_events: vec![
                        access_event(cell(1, 0, 0), OpKind::Read, u64_portable(20), 4, 0),
                        access_event(cell(1, 1, 0), OpKind::Read, u64_portable(80), 5, 1),
                    ],
                    failed_instruction: Some(3),
                },
                TxResult::Success {
                    emitted: vec![],
                    access_trace: vec![
                        access_event(cell(1, 0, 0), OpKind::Read, u64_portable(20), 4, 0),
                        access_event(cell(1, 1, 0), OpKind::Read, u64_portable(80), 5, 1),
                        access_event(cell(1, 0, 0), OpKind::Write, u64_portable(10), 6, 2),
                        access_event(cell(1, 1, 0), OpKind::Write, u64_portable(90), 7, 3),
                    ],
                    precompile_events: vec![],
                    property_reads: vec![],
                },
            ],
        }
    );
    assert!(check_journal_consistency(&journal).is_ok());
}

#[test]
fn hash_instruction_projection_matches_journal_shape() {
    let snap = TestSnapshot(BTreeMap::new());
    let sender = [1u8; 32];
    let batch = Batch {
        transactions: vec![make_tx(4, vec![], sender, 0)],
    };
    let mut prog = test_program();
    prog.register(hash_tx_def()).unwrap();

    let env = test_env();
    let journal = execute_journal(&batch, &prog, &snap, &env, &BTreeMap::new());
    let shard = journal.successful_txs().next().expect("successful tx");
    assert_eq!(shard.ir_hash_calls.len(), 1);
    assert_eq!(shard.ir_hash_calls[0].instruction_index, 0);
    assert_eq!(shard.ir_hash_calls[0].inputs, vec![u64_portable(7)]);
    assert_eq!(
        shard.ir_hash_calls[0].digest,
        bytes32_portable(XorHasher.hash_ir(&[u64_portable(7)])),
    );
    let result = derive_batch_report(&journal, env.type_runtimes).expect("derive batch result");
    assert_eq!(
        result,
        BatchReport {
            read_set_old: vec![],
            write_set_final: vec![],
            txs: vec![TxResult::Success {
                emitted: vec![],
                access_trace: vec![],
                precompile_events: vec![],
                property_reads: vec![],
            }],
        }
    );
}

#[test]
fn property_read_projection_matches_journal_shape() {
    let snap = TestSnapshot(BTreeMap::new());
    let sender = [1u8; 32];
    let batch = Batch {
        transactions: vec![make_tx(5, vec![], sender, 0)],
    };
    let mut prog = test_program();
    prog.register(property_read_tx_def()).unwrap();

    let committed_state = MockCommittedState {
        columns: BTreeMap::from([(
            (TableId(1), ColId(0)),
            vec![
                TypedColumnEntry {
                    row_key: RowKey(9),
                    value: u64_typed(90),
                    is_null: false,
                },
                TypedColumnEntry {
                    row_key: RowKey(4),
                    value: u64_typed(40),
                    is_null: false,
                },
            ],
        )]),
    };
    let mut property_queries = PropertyQueryRegistry::new();
    property_queries
        .register(TableId(1), ColId(0), Box::new(MinimumResolver))
        .expect("register property query");
    let env = tabula_executor::batch::BatchEnv {
        committed_state: Some(&committed_state),
        property_queries: &property_queries,
        ..test_env()
    };

    let journal = execute_journal(&batch, &prog, &snap, &env, &BTreeMap::new());
    let result = derive_batch_report(&journal, env.type_runtimes).expect("derive batch result");
    assert_eq!(
        result,
        BatchReport {
            read_set_old: vec![],
            write_set_final: vec![],
            txs: vec![TxResult::Success {
                emitted: vec![],
                access_trace: vec![],
                precompile_events: vec![],
                property_reads: vec![PropertyReadResult {
                    instruction_index: 0,
                    value: u64_portable(40),
                    key: Some(RowKey(4)),
                    is_null: false,
                }],
            }],
        }
    );
}

#[test]
fn precompile_projection_matches_journal_shape() {
    let snap = TestSnapshot(BTreeMap::new());
    let sender = [1u8; 32];
    let batch = Batch {
        transactions: vec![make_tx(1, vec![], sender, 0)],
    };
    let prog = compiled_precompile_requirement_program().program().clone();

    let mut precompiles = PrecompileRegistry::new();
    precompiles
        .register(ConstantOnePrecompileHandler::default())
        .expect("register handler");
    let env = tabula_executor::batch::BatchEnv {
        precompiles: Some(&precompiles),
        ..test_env()
    };

    let journal = execute_journal(&batch, &prog, &snap, &env, &BTreeMap::new());
    let result = derive_batch_report(&journal, env.type_runtimes).expect("derive batch result");
    assert_eq!(
        result,
        BatchReport {
            read_set_old: vec![],
            write_set_final: vec![],
            txs: vec![TxResult::Success {
                emitted: vec![],
                access_trace: vec![],
                precompile_events: vec![PrecompileEvent {
                    tx_index: 0,
                    instruction_index: 0,
                    precompile_id: 0x0001,
                    inputs: vec![],
                    outputs: vec![u64_portable(1)],
                }],
                property_reads: vec![],
            }],
        }
    );
}

#[test]
fn invalid_signature() {
    let snap = TestSnapshot(BTreeMap::new());
    let batch = Batch {
        transactions: vec![make_tx(
            1,
            vec![u64_portable(0), u64_portable(1)],
            [1u8; 32],
            0,
        )],
    };
    let mut prog = test_program();
    prog.register(write_tx_def()).unwrap();

    let env = tabula_executor::batch::BatchEnv {
        sig_verifier: &AlwaysInvalidSig,
        ..test_env()
    };
    let result = execute_journal(&batch, &prog, &snap, &env, &BTreeMap::new());
    assert!(matches!(result.txs[0], TxExecutionOutcome::Failed(_)));
}

#[test]
fn invalid_nonce() {
    let snap = TestSnapshot(BTreeMap::new());
    let batch = Batch {
        transactions: vec![make_tx(
            1,
            vec![u64_portable(0), u64_portable(1)],
            [1u8; 32],
            999,
        )],
    };
    let mut prog = test_program();
    prog.register(write_tx_def()).unwrap();

    let env = test_env();
    let result = execute_journal(&batch, &prog, &snap, &env, &BTreeMap::new());
    assert!(matches!(result.txs[0], TxExecutionOutcome::Failed(_)));
}

#[test]
fn empty_batch() {
    let snap = TestSnapshot(BTreeMap::new());
    let batch = Batch {
        transactions: vec![],
    };
    let prog = tabula_ir::Program::new();

    let env = test_env();
    let result = execute_journal(&batch, &prog, &snap, &env, &BTreeMap::new());
    assert!(result.txs.is_empty());
    assert!(result.state_summary.read_set_old.is_empty());
    assert!(result.state_summary.write_set_final.is_empty());
}

#[test]
fn tx_outcomes_len_matches_batch() {
    let snap = TestSnapshot(BTreeMap::new());
    let sender = [1u8; 32];
    let batch = Batch {
        transactions: vec![
            make_tx(1, vec![u64_portable(0), u64_portable(1)], sender, 0),
            make_tx(1, vec![u64_portable(1), u64_portable(2)], sender, 1),
            make_tx(1, vec![u64_portable(2), u64_portable(3)], sender, 2),
        ],
    };
    let mut prog = test_program();
    prog.register(write_tx_def()).unwrap();

    let env = test_env();
    let result = execute_journal(&batch, &prog, &snap, &env, &BTreeMap::new());
    assert_eq!(result.txs.len(), batch.transactions.len());
}

#[test]
fn param_count_mismatch_fails() {
    let snap = TestSnapshot(BTreeMap::new());
    let sender = [1u8; 32];
    let batch = Batch {
        transactions: vec![make_tx(1, vec![u64_portable(0)], sender, 0)],
    };
    let mut prog = test_program();
    prog.register(write_tx_def()).unwrap();

    let env = test_env();
    let result = execute_journal(&batch, &prog, &snap, &env, &BTreeMap::new());
    assert!(
        matches!(&result.txs[0], TxExecutionOutcome::Failed(failure) if failure.reason.contains("expected 2 params"))
    );
}

#[test]
fn param_type_mismatch_fails() {
    let snap = TestSnapshot(BTreeMap::new());
    let sender = [1u8; 32];
    let batch = Batch {
        transactions: vec![make_tx(
            1,
            vec![u64_portable(0), bool_portable(true)],
            sender,
            0,
        )],
    };
    let mut prog = test_program();
    prog.register(write_tx_def()).unwrap();

    let env = test_env();
    let result = execute_journal(&batch, &prog, &snap, &env, &BTreeMap::new());
    assert!(
        matches!(&result.txs[0], TxExecutionOutcome::Failed(failure) if failure.reason.contains("param 1"))
    );
}

#[test]
fn events_carry_correct_tx_index() {
    let snap = TestSnapshot(BTreeMap::new());
    let sender = [1u8; 32];
    let batch = Batch {
        transactions: vec![
            make_tx(1, vec![u64_portable(0), u64_portable(10)], sender, 0),
            make_tx(1, vec![u64_portable(1), u64_portable(20)], sender, 1),
            make_tx(1, vec![u64_portable(2), u64_portable(30)], sender, 2),
        ],
    };
    let mut prog = test_program();
    prog.register(write_tx_def()).unwrap();

    let env = test_env();
    let result = execute_journal(&batch, &prog, &snap, &env, &BTreeMap::new());
    let indices: Vec<u32> = result
        .successful_access_effects_with_tx()
        .map(|(tx_idx, _)| tx_idx)
        .collect();
    assert_eq!(indices, vec![0, 1, 2]);
}

#[test]
fn failed_tx_partial_events() {
    let mut data = BTreeMap::new();
    data.insert(cell(1, 0, 0), u64_portable(10));
    data.insert(cell(1, 1, 0), u64_portable(50));
    let snap = snapshot(data);
    let sender = [1u8; 32];

    let batch = Batch {
        transactions: vec![make_tx(2, vec![u64_portable(100)], sender, 0)],
    };
    let mut prog = test_program();
    prog.register(transfer_tx_def()).unwrap();

    let env = test_env();
    let result = execute_journal(&batch, &prog, &snap, &env, &BTreeMap::new());
    match &result.txs[0] {
        TxExecutionOutcome::Failed(failure) => {
            assert_eq!(failure.partial_accesses.len(), 2);
            assert_eq!(failure.failed_instruction, Some(3));
            assert_eq!(failure.partial_accesses[0].effect_ordinal_in_tx, 0);
            assert_eq!(failure.partial_accesses[0].attempt_time, 0);
            assert_eq!(failure.partial_accesses[1].effect_ordinal_in_tx, 1);
            assert_eq!(failure.partial_accesses[1].attempt_time, 1);
        }
        TxExecutionOutcome::Success(_) => panic!("expected failure"),
    }
}

#[test]
fn precheck_failure_empty_partial() {
    let snap = TestSnapshot(BTreeMap::new());
    let sender = [1u8; 32];
    let batch = Batch {
        transactions: vec![make_tx(1, vec![u64_portable(0)], sender, 0)],
    };
    let mut prog = test_program();
    prog.register(write_tx_def()).unwrap();

    let env = test_env();
    let result = execute_journal(&batch, &prog, &snap, &env, &BTreeMap::new());
    match &result.txs[0] {
        TxExecutionOutcome::Failed(failure) => {
            assert!(failure.partial_accesses.is_empty());
            assert_eq!(failure.failed_instruction, None);
        }
        TxExecutionOutcome::Success(_) => panic!("expected failure"),
    }
}

#[test]
fn multi_sender_independent_nonces() {
    let snap = TestSnapshot(BTreeMap::new());
    let sender_a = [1u8; 32];
    let sender_b = [2u8; 32];
    let batch = Batch {
        transactions: vec![
            make_tx(1, vec![u64_portable(0), u64_portable(10)], sender_a, 0),
            make_tx(1, vec![u64_portable(1), u64_portable(20)], sender_b, 0),
            make_tx(1, vec![u64_portable(2), u64_portable(30)], sender_a, 1),
            make_tx(1, vec![u64_portable(3), u64_portable(40)], sender_b, 1),
        ],
    };
    let mut prog = test_program();
    prog.register(write_tx_def()).unwrap();

    let env = test_env();
    let result = execute_journal(&batch, &prog, &snap, &env, &BTreeMap::new());
    assert_eq!(result.txs.len(), 4);
    assert!(result.txs.iter().all(|record| record.success().is_some()));
}

#[test]
fn failed_tx_reads_not_in_read_set() {
    let mut data = BTreeMap::new();
    data.insert(cell(1, 0, 0), u64_portable(50));
    data.insert(cell(1, 1, 0), u64_portable(50));
    let snap = snapshot(data);
    let sender = [1u8; 32];

    let batch = Batch {
        transactions: vec![
            make_tx(2, vec![u64_portable(10)], sender, 0),
            make_tx(2, vec![u64_portable(999)], sender, 1),
        ],
    };
    let mut prog = test_program();
    prog.register(transfer_tx_def()).unwrap();

    let env = test_env();
    let result = execute_journal(&batch, &prog, &snap, &env, &BTreeMap::new());
    assert!(result.txs[0].success().is_some());
    assert!(matches!(result.txs[1], TxExecutionOutcome::Failed(_)));

    let read_keys: Vec<CellKey> = result
        .state_summary
        .read_set_old
        .iter()
        .map(|entry| entry.key)
        .collect();
    assert!(read_keys.contains(&cell(1, 0, 0)));
    assert!(read_keys.contains(&cell(1, 1, 0)));
    let unique: std::collections::BTreeSet<CellKey> = read_keys.iter().copied().collect();
    assert_eq!(
        read_keys.len(),
        unique.len(),
        "no duplicate keys in read_set_old"
    );
}

#[test]
fn emitted_events_accumulate_across_txs() {
    let snap = TestSnapshot(BTreeMap::new());
    let sender = [1u8; 32];

    let emit_def = TxTypeDef {
        id: TxTypeId(10),
        name: "emit_test".into(),
        param_schema: vec![param("val", TYPE_U64_ID)],
        body: vec![Instruction::Emit {
            topic: b"event".to_vec(),
            data: vec![ValueExpr::Param(0)],
        }],
    };

    let batch = Batch {
        transactions: vec![
            make_tx(10, vec![u64_portable(1)], sender, 0),
            make_tx(10, vec![u64_portable(2)], sender, 1),
            make_tx(10, vec![u64_portable(3)], sender, 2),
        ],
    };
    let mut prog = tabula_ir::Program::new();
    prog.register(emit_def).unwrap();

    let env = test_env();
    let result = execute_journal(&batch, &prog, &snap, &env, &BTreeMap::new());
    let emitted: Vec<_> = result.successful_emitted().collect();
    assert_eq!(emitted.len(), 3);
    assert_eq!(emitted[0].data, vec![portable(u64_portable(1))]);
    assert_eq!(emitted[1].data, vec![portable(u64_portable(2))]);
    assert_eq!(emitted[2].data, vec![portable(u64_portable(3))]);
}

#[test]
fn emitted_event_projection_matches_journal_exactly() {
    let snap = TestSnapshot(BTreeMap::new());
    let sender = [1u8; 32];
    let emit_def = TxTypeDef {
        id: TxTypeId(10),
        name: "emit_test".into(),
        param_schema: vec![param("val", TYPE_U64_ID)],
        body: vec![Instruction::Emit {
            topic: b"event".to_vec(),
            data: vec![ValueExpr::Param(0)],
        }],
    };
    let batch = Batch {
        transactions: vec![make_tx(10, vec![u64_portable(7)], sender, 0)],
    };
    let mut prog = tabula_ir::Program::new();
    prog.register(emit_def).unwrap();

    let env = test_env();
    let journal = execute_journal(&batch, &prog, &snap, &env, &BTreeMap::new());
    let result = derive_batch_report(&journal, env.type_runtimes).expect("derive batch result");
    assert_eq!(
        result,
        BatchReport {
            read_set_old: vec![],
            write_set_final: vec![],
            txs: vec![TxResult::Success {
                emitted: vec![EmittedEvent {
                    topic: b"event".to_vec(),
                    data: vec![u64_portable(7)],
                }],
                access_trace: vec![],
                precompile_events: vec![],
                property_reads: vec![],
            }],
        }
    );
}

#[test]
fn successful_logical_time_remains_monotonic_across_failures() {
    let mut data = BTreeMap::new();
    data.insert(cell(1, 0, 0), u64_portable(50));
    data.insert(cell(1, 1, 0), u64_portable(50));
    let snap = snapshot(data);
    let sender = [1u8; 32];
    let batch = Batch {
        transactions: vec![
            make_tx(2, vec![u64_portable(30)], sender, 0),
            make_tx(2, vec![u64_portable(100)], sender, 1),
            make_tx(2, vec![u64_portable(10)], sender, 1),
        ],
    };
    let mut prog = test_program();
    prog.register(transfer_tx_def()).unwrap();

    let env = test_env();
    let journal = execute_journal(&batch, &prog, &snap, &env, &BTreeMap::new());
    let logical_times: Vec<_> = journal
        .successful_access_effects()
        .map(|effect| effect.logical_time)
        .collect();
    assert_eq!(logical_times, vec![0, 1, 2, 3, 4, 5, 6, 7]);
    match &journal.txs[1] {
        TxExecutionOutcome::Failed(failure) => {
            let attempt_times: Vec<_> = failure
                .partial_accesses
                .iter()
                .map(|effect| effect.attempt_time)
                .collect();
            assert_eq!(attempt_times, vec![4, 5]);
        }
        TxExecutionOutcome::Success(_) => panic!("expected failure"),
    }
}

#[test]
fn unknown_tx_type_fails() {
    let snap = TestSnapshot(BTreeMap::new());
    let sender = [1u8; 32];
    let batch = Batch {
        transactions: vec![make_tx(999, vec![], sender, 0)],
    };
    let prog = tabula_ir::Program::new();

    let env = test_env();
    let result = execute_journal(&batch, &prog, &snap, &env, &BTreeMap::new());
    assert!(matches!(result.txs[0], TxExecutionOutcome::Failed(_)));
}
