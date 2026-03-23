//! Batch executor integration tests.

mod common;

use std::collections::BTreeMap;

use tabula_core::{Batch, CellKey, ColId, RowKey, TableId, TxResult, TxTypeId, TypeId};
use tabula_ir::{ArithOp, CmpOp, Instruction, ParamDef, RowExpr, TxTypeDef, ValueExpr};
use tabula_profile::TYPE_U64_ID;
use tabula_testing::assertions::{ExpectedTxOutcome, assert_tx_outcomes, assert_write_set_cell};

use tabula_executor::batch::execute_batch;

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

    let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
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

    let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
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

    let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
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
    let result = execute_batch(&batch, &prog, &snap, &env, &BTreeMap::new()).unwrap();
    assert!(matches!(result.txs[0], TxResult::Failed { .. }));
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

    let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
    assert!(matches!(result.txs[0], TxResult::Failed { .. }));
}

#[test]
fn empty_batch() {
    let snap = TestSnapshot(BTreeMap::new());
    let batch = Batch {
        transactions: vec![],
    };
    let prog = tabula_ir::Program::new();

    let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
    assert!(result.txs.is_empty());
    assert!(result.read_set_old.is_empty());
    assert!(result.write_set_final.is_empty());
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

    let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
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

    let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
    assert!(
        matches!(&result.txs[0], TxResult::Failed { reason, .. } if reason.contains("expected 2 params"))
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

    let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
    assert!(
        matches!(&result.txs[0], TxResult::Failed { reason, .. } if reason.contains("param 1"))
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

    let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
    let indices: Vec<u32> = result
        .successful_events_with_tx()
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

    let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
    match &result.txs[0] {
        TxResult::Failed {
            partial_events,
            failed_instruction,
            ..
        } => {
            assert_eq!(partial_events.len(), 2);
            assert_eq!(*failed_instruction, Some(3));
        }
        TxResult::Success { .. } => panic!("expected failure"),
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

    let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
    match &result.txs[0] {
        TxResult::Failed {
            partial_events,
            failed_instruction,
            ..
        } => {
            assert!(partial_events.is_empty());
            assert_eq!(*failed_instruction, None);
        }
        TxResult::Success { .. } => panic!("expected failure"),
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

    let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
    assert_eq!(result.txs.len(), 4);
    assert!(result.txs.iter().all(TxResult::is_success));
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

    let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
    assert!(result.txs[0].is_success());
    assert!(matches!(result.txs[1], TxResult::Failed { .. }));

    let read_keys: Vec<CellKey> = result.read_set_old.iter().map(|(k, _)| *k).collect();
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

    let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
    let emitted: Vec<_> = result.successful_emitted().collect();
    assert_eq!(emitted.len(), 3);
    assert_eq!(emitted[0].data, vec![portable(u64_portable(1))]);
    assert_eq!(emitted[1].data, vec![portable(u64_portable(2))]);
    assert_eq!(emitted[2].data, vec![portable(u64_portable(3))]);
}

#[test]
fn unknown_tx_type_fails() {
    let snap = TestSnapshot(BTreeMap::new());
    let sender = [1u8; 32];
    let batch = Batch {
        transactions: vec![make_tx(999, vec![], sender, 0)],
    };
    let prog = tabula_ir::Program::new();

    let result = execute_batch(&batch, &prog, &snap, &test_env(), &BTreeMap::new()).unwrap();
    assert!(matches!(result.txs[0], TxResult::Failed { .. }));
}
