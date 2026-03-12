//! Integration tests: multi-tx batch execution with mixed outcomes and determinism.

use tabula_core::mock::*;
use tabula_core::{
    Batch, CellKey, ColId, ColumnDef, RowKey, TableId, TableSchema, Transaction, TxOutcome,
    TxTypeId, Value, ValueType,
};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_executor::consistency::check_consistency;
use tabula_ir::{ArithOp, CmpOp, Instruction, ParamDef, Program, RowExpr, TxTypeDef, ValueExpr};

/// NF-compliant transfer: reads `from_row` and `to_row` of (table 1, col 0),
/// transfers `amount` (param 0) from `from_row` to `to_row`.
fn transfer_def(id: u32, from_row: u64, to_row: u64) -> TxTypeDef {
    TxTypeDef {
        id: TxTypeId(id),
        name: format!("transfer_{from_row}_to_{to_row}"),
        param_schema: vec![ParamDef {
            name: "amount".into(),
            value_type: ValueType::U64,
        }],
        body: vec![
            Instruction::Read {
                dst_val: 0,
                dst_is_null: 1,
                table: TableId(1),
                row: RowExpr::Literal(RowKey(from_row)),
                col: ColId(0),
            },
            Instruction::Read {
                dst_val: 2,
                dst_is_null: 3,
                table: TableId(1),
                row: RowExpr::Literal(RowKey(to_row)),
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
                row: RowExpr::Literal(RowKey(from_row)),
                col: ColId(0),
                src_val: ValueExpr::Slot(5),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
            Instruction::Write {
                table: TableId(1),
                row: RowExpr::Literal(RowKey(to_row)),
                col: ColId(0),
                src_val: ValueExpr::Slot(6),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
        ],
    }
}

fn make_tx(tx_type_id: u32, amount: u64, nonce: u64) -> Transaction {
    Transaction {
        tx_type: TxTypeId(tx_type_id),
        params: vec![Value::U64(amount)],
        sender: [1u8; 32],
        nonce,
        signature: vec![],
    }
}

fn setup_state() -> InMemoryState {
    let mut state = InMemoryState::new();
    state.set(
        CellKey {
            table: TableId(1),
            col: ColId(0),
            row: RowKey(0),
        },
        Value::U64(1000),
    );
    state.set(
        CellKey {
            table: TableId(1),
            col: ColId(0),
            row: RowKey(1),
        },
        Value::U64(500),
    );
    state.set(
        CellKey {
            table: TableId(1),
            col: ColId(0),
            row: RowKey(2),
        },
        Value::U64(200),
    );
    state
}

#[test]
fn test_multi_tx_mixed_outcomes() {
    let state = setup_state();
    let mut prog = Program::new();
    prog.add_schema(TableSchema {
        id: TableId(1),
        name: "test".into(),
        columns: vec![ColumnDef {
            id: ColId(0),
            name: "val".into(),
            value_type: ValueType::U64,
        }],
    });
    prog.register(transfer_def(1, 0, 1)).unwrap(); // 0→1
    prog.register(transfer_def(2, 0, 2)).unwrap(); // 0→2
    prog.register(transfer_def(3, 1, 2)).unwrap(); // 1→2

    let batch = Batch {
        transactions: vec![
            make_tx(1, 300, 0), // OK: Alice 1000 -> 700, Bob 500 -> 800
            make_tx(2, 800, 1), // FAIL: Alice only has 700
            make_tx(3, 100, 1), // OK (nonce 1 since tx1 failed): Bob 800 -> 700, Charlie 200 -> 300
        ],
    };

    let st = InMemoryStaticTables::new();
    let env = BatchEnv {
        hasher: &MockHasher,
        sig_verifier: &MockSigVerifier,
        nonce_policy: &SequentialNonce,
        static_tables: &st,
        precompiles: None,
    };
    let result = execute_batch(
        &batch,
        &prog,
        &state,
        &env,
        &std::collections::BTreeMap::new(),
    )
    .unwrap();

    assert_eq!(result.tx_outcomes[0], TxOutcome::Success);
    assert!(matches!(result.tx_outcomes[1], TxOutcome::Failed { .. }));
    assert_eq!(result.tx_outcomes[2], TxOutcome::Success);

    // Final: Alice = 700, Bob = 700, Charlie = 300
    let ws: std::collections::BTreeMap<_, _> = result.write_set_final.iter().copied().collect();
    assert_eq!(
        ws[&CellKey {
            table: TableId(1),
            col: ColId(0),
            row: RowKey(0),
        }],
        Some(Value::U64(700))
    );
    assert_eq!(
        ws[&CellKey {
            table: TableId(1),
            col: ColId(0),
            row: RowKey(1),
        }],
        Some(Value::U64(700))
    );
    assert_eq!(
        ws[&CellKey {
            table: TableId(1),
            col: ColId(0),
            row: RowKey(2),
        }],
        Some(Value::U64(300))
    );
}

#[test]
fn test_deterministic_execution() {
    let state = setup_state();
    let mut prog = Program::new();
    prog.add_schema(TableSchema {
        id: TableId(1),
        name: "test".into(),
        columns: vec![ColumnDef {
            id: ColId(0),
            name: "val".into(),
            value_type: ValueType::U64,
        }],
    });
    prog.register(transfer_def(1, 0, 1)).unwrap();
    prog.register(transfer_def(2, 1, 2)).unwrap();

    let batch = Batch {
        transactions: vec![make_tx(1, 100, 0), make_tx(2, 50, 1)],
    };

    let st = InMemoryStaticTables::new();
    let env = BatchEnv {
        hasher: &MockHasher,
        sig_verifier: &MockSigVerifier,
        nonce_policy: &SequentialNonce,
        static_tables: &st,
        precompiles: None,
    };
    let r1 = execute_batch(
        &batch,
        &prog,
        &state,
        &env,
        &std::collections::BTreeMap::new(),
    )
    .unwrap();

    let r2 = execute_batch(
        &batch,
        &prog,
        &state,
        &env,
        &std::collections::BTreeMap::new(),
    )
    .unwrap();

    assert_eq!(r1.read_set_old, r2.read_set_old);
    assert_eq!(r1.write_set_final, r2.write_set_final);
    assert_eq!(r1.events, r2.events);
    assert_eq!(r1.tx_outcomes, r2.tx_outcomes);
}

#[test]
fn test_consistency_passes_for_valid_batch() {
    let state = setup_state();
    let mut prog = Program::new();
    prog.add_schema(TableSchema {
        id: TableId(1),
        name: "test".into(),
        columns: vec![ColumnDef {
            id: ColId(0),
            name: "val".into(),
            value_type: ValueType::U64,
        }],
    });
    prog.register(transfer_def(1, 0, 1)).unwrap();
    prog.register(transfer_def(2, 1, 2)).unwrap();
    prog.register(transfer_def(3, 2, 0)).unwrap();

    let batch = Batch {
        transactions: vec![make_tx(1, 100, 0), make_tx(2, 50, 1), make_tx(3, 25, 2)],
    };

    let st = InMemoryStaticTables::new();
    let env = BatchEnv {
        hasher: &MockHasher,
        sig_verifier: &MockSigVerifier,
        nonce_policy: &SequentialNonce,
        static_tables: &st,
        precompiles: None,
    };
    let result = execute_batch(
        &batch,
        &prog,
        &state,
        &env,
        &std::collections::BTreeMap::new(),
    )
    .unwrap();

    assert!(result.tx_outcomes.iter().all(|o| *o == TxOutcome::Success));
    assert!(check_consistency(&result.events, &result.read_set_old).is_ok());
}
