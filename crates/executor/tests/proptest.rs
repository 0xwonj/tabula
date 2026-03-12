//! Property-based tests for overlay semantics and consistency.

mod common;

use std::collections::BTreeMap;

use proptest::prelude::*;

use tabula_core::{
    Batch, ColId, ColumnDef, AccessEvent, OpKind, RowKey, TableId, TableSchema, Transaction,
    TxTypeId, Value, ValueType,
};
use tabula_ir::{ArithOp, Instruction, ParamDef, RowExpr, TxTypeDef, ValueExpr};

use tabula_executor::consistency::check_consistency;
use tabula_executor::overlay::Overlay;

use common::*;

/// Proptest-specific shorthand: single-column cell by row.
fn pcell(r: u64) -> tabula_core::CellKey {
    cell(1, r, 0)
}

// ── Strategies ──────────────────────────────────────────────────────────

fn arb_value() -> impl Strategy<Value = Option<Value>> {
    prop_oneof![any::<u64>().prop_map(|n| Some(Value::U64(n))), Just(None)]
}

fn arb_row() -> impl Strategy<Value = u64> {
    0u64..10
}

// ── Properties ──────────────────────────────────────────────────────────

proptest! {
    #[test]
    fn prop_read_after_write_returns_last(
        writes in proptest::collection::vec((arb_row(), arb_value()), 1..20),
        read_row in arb_row(),
    ) {
        let snap = TestSnapshot(BTreeMap::new());
        let mut ov = Overlay::new(&snap);

        for (r, v) in &writes {
            ov.write(&pcell(*r), *v, ValueType::U64);
        }

        let result = ov.read(&pcell(read_row), ValueType::U64).unwrap();
        let expected: Option<Value> = writes.iter()
            .rev()
            .find(|(r, _)| *r == read_row)
            .and_then(|(_, v)| *v);

        prop_assert_eq!(result, expected);
    }
}

proptest! {
    #[test]
    fn prop_write_set_one_per_key(
        writes in proptest::collection::vec((arb_row(), arb_value()), 1..30),
    ) {
        let snap = TestSnapshot(BTreeMap::new());
        let mut ov = Overlay::new(&snap);

        for (r, v) in &writes {
            ov.write(&pcell(*r), *v, ValueType::U64);
        }

        let result = ov.into_result();
        let keys: Vec<tabula_core::CellKey> = result.write_set_final.iter().map(|(k, _)| *k).collect();
        let unique: std::collections::BTreeSet<tabula_core::CellKey> = keys.iter().copied().collect();
        prop_assert_eq!(keys.len(), unique.len(), "duplicate keys in write_set_final");
    }
}

proptest! {
    #[test]
    fn prop_read_set_old_unique_keys(
        reads in proptest::collection::vec(arb_row(), 1..20),
    ) {
        let mut data = BTreeMap::new();
        for r in 0..10u64 {
            data.insert(pcell(r), Value::U64(r * 10));
        }
        let snap = TestSnapshot(data);
        let mut ov = Overlay::new(&snap);

        for r in &reads {
            let _ = ov.read(&pcell(*r), ValueType::U64).unwrap();
        }

        let result = ov.into_result();
        let keys: Vec<tabula_core::CellKey> = result.read_set_old.iter().map(|(k, _)| *k).collect();
        let unique: std::collections::BTreeSet<tabula_core::CellKey> = keys.iter().copied().collect();
        prop_assert_eq!(keys.len(), unique.len(), "duplicate keys in read_set_old");
    }
}

proptest! {
    #[test]
    fn prop_checkpoint_rollback_preserves(
        pre_writes in proptest::collection::vec((arb_row(), arb_value()), 0..10),
        post_writes in proptest::collection::vec((arb_row(), arb_value()), 1..10),
        read_row in arb_row(),
    ) {
        let snap = TestSnapshot(BTreeMap::new());
        let mut ov = Overlay::new(&snap);

        for (r, v) in &pre_writes {
            ov.write(&pcell(*r), *v, ValueType::U64);
        }

        let expected = ov.read(&pcell(read_row), ValueType::U64).unwrap();

        ov.checkpoint();
        for (r, v) in &post_writes {
            ov.write(&pcell(*r), *v, ValueType::U64);
        }

        ov.rollback();

        let actual = ov.read(&pcell(read_row), ValueType::U64).unwrap();
        prop_assert_eq!(actual, expected);
    }
}

proptest! {
    #[test]
    fn prop_write_before_read_excludes_from_read_set(
        write_rows in proptest::collection::vec(arb_row(), 1..5),
        read_rows in proptest::collection::vec(arb_row(), 1..5),
    ) {
        let mut data = BTreeMap::new();
        for r in 0..10u64 {
            data.insert(pcell(r), Value::U64(r));
        }
        let snap = TestSnapshot(data);
        let mut ov = Overlay::new(&snap);

        for r in &write_rows {
            ov.write(&pcell(*r), Some(Value::U64(999)), ValueType::U64);
        }
        for r in &read_rows {
            let _ = ov.read(&pcell(*r), ValueType::U64).unwrap();
        }

        let result = ov.into_result();
        let read_keys: std::collections::BTreeSet<tabula_core::CellKey> =
            result.read_set_old.iter().map(|(k, _)| *k).collect();
        let write_keys: std::collections::BTreeSet<tabula_core::CellKey> =
            write_rows.iter().map(|r| pcell(*r)).collect();

        for k in &write_keys {
            prop_assert!(
                !read_keys.contains(k),
                "key {:?} was written before read but appears in read_set_old", k
            );
        }
    }
}

proptest! {
    #[test]
    fn prop_interpreter_events_consistent(
        amounts in proptest::collection::vec(1u64..50, 1..5),
    ) {
        let initial_balance = 1000u64;
        let mut data = BTreeMap::new();
        data.insert(pcell(0), Value::U64(initial_balance));
        let snap = TestSnapshot(data);

        let mut prog = tabula_ir::Program::new();
        prog.add_schema(TableSchema {
            id: TableId(1),
            name: "test".into(),
            columns: vec![ColumnDef {
                id: ColId(0),
                name: "val".into(),
                value_type: ValueType::U64,
            }],
        });
        prog.register(TxTypeDef {
            id: TxTypeId(1),
            name: "withdraw".into(),
            param_schema: vec![ParamDef { name: "amount".into(), value_type: ValueType::U64 }],
            body: vec![
                Instruction::Read {
                    dst_val: 0, dst_is_null: 1,
                    table: TableId(1), row: RowExpr::Literal(RowKey(0)), col: ColId(0),
                },
                Instruction::Arith {
                    dst: 2, op: ArithOp::Sub,
                    lhs: ValueExpr::Slot(0), rhs: ValueExpr::Param(0),
                },
                Instruction::Write {
                    table: TableId(1), row: RowExpr::Literal(RowKey(0)), col: ColId(0),
                    src_val: ValueExpr::Slot(2),
                    src_is_null: ValueExpr::Literal(Value::Bool(false)),
                },
            ],
        }).unwrap();

        let sender = [1u8; 32];
        let txs: Vec<Transaction> = amounts.iter().enumerate()
            .map(|(i, &amt)| Transaction {
                tx_type: TxTypeId(1),
                params: vec![Value::U64(amt)],
                sender,
                nonce: i as u64,
                signature: vec![],
            })
            .collect();
        let batch = Batch { transactions: txs };

        let env = tabula_executor::batch::BatchEnv {
            hasher: &XorHasher,
            sig_verifier: &AlwaysValidSig,
            nonce_policy: &SeqNonce,
            static_tables: &EmptyStaticTables,
            precompiles: None,
        };
        let result = tabula_executor::batch::execute_batch(
            &batch, &prog, &snap, &env, &BTreeMap::new(),
        ).unwrap();

        let events: Vec<_> = result.successful_events().cloned().collect();
        let check = check_consistency(&events, &result.read_set_old);
        prop_assert!(check.is_ok(), "consistency check failed: {:?}", check.err());
    }
}

proptest! {
    #[test]
    fn prop_tampered_trace_fails(
        tamper_idx in 0usize..5,
    ) {
        let k = pcell(0);
        let mut events = vec![
            AccessEvent { key: k, op: OpKind::Read, value: Value::U64(100), val_is_null: false, time: 0, tx_index: 0, effect_ordinal_in_tx: 0 },
            AccessEvent { key: k, op: OpKind::Write, value: Value::U64(80), val_is_null: false, time: 1, tx_index: 0, effect_ordinal_in_tx: 1 },
            AccessEvent { key: k, op: OpKind::Read, value: Value::U64(80), val_is_null: false, time: 2, tx_index: 0, effect_ordinal_in_tx: 2 },
            AccessEvent { key: k, op: OpKind::Write, value: Value::U64(60), val_is_null: false, time: 3, tx_index: 0, effect_ordinal_in_tx: 3 },
            AccessEvent { key: k, op: OpKind::Read, value: Value::U64(60), val_is_null: false, time: 4, tx_index: 0, effect_ordinal_in_tx: 4 },
        ];
        let read_set_old = vec![(k, Some(Value::U64(100)))];

        let idx = tamper_idx % events.len();
        events[idx].value = Value::U64(999);

        let check = check_consistency(&events, &read_set_old);
        prop_assert!(check.is_err(), "tampered trace should fail consistency check");
    }
}
