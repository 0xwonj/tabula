//! Property-based tests for overlay semantics and consistency.

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use proptest::prelude::*;

    use tabula_core::event::{ExecutionEvent, OpKind};
    use tabula_core::ir::{Instruction, RowExpr, ValueExpr};
    use tabula_core::tx::{Batch, ParamDef, Transaction, TxTypeDef, TxTypeId};
    use tabula_core::types::*;

    use crate::consistency::check_consistency;
    use crate::overlay::Overlay;
    use crate::test_fixtures::*;

    /// Proptest-specific shorthand: single-column cell by row.
    fn pcell(r: u64) -> CellKey {
        cell(1, r, 0)
    }

    // --- Proptest strategies ---

    fn arb_value() -> impl Strategy<Value = Value> {
        prop_oneof![any::<u64>().prop_map(Value::U64), Just(Value::Null),]
    }

    fn arb_row() -> impl Strategy<Value = u64> {
        0u64..10
    }

    // --- Property 1: Read-after-write always returns last written value ---
    proptest! {
        #[test]
        fn prop_read_after_write_returns_last(
            writes in proptest::collection::vec((arb_row(), arb_value()), 1..20),
            read_row in arb_row(),
        ) {
            let snap = TestSnapshot(BTreeMap::new());
            let mut ov = Overlay::new(&snap);

            for (r, v) in &writes {
                ov.write(&pcell(*r), v.clone());
            }

            let result = ov.read(&pcell(read_row)).unwrap();

            let expected = writes.iter()
                .rev()
                .find(|(r, _)| *r == read_row)
                .map(|(_, v)| v.clone())
                .unwrap_or(Value::Null);

            prop_assert_eq!(result, expected);
        }
    }

    // --- Property 2: write_set_final has exactly one entry per written key ---
    proptest! {
        #[test]
        fn prop_write_set_one_per_key(
            writes in proptest::collection::vec((arb_row(), arb_value()), 1..30),
        ) {
            let snap = TestSnapshot(BTreeMap::new());
            let mut ov = Overlay::new(&snap);

            for (r, v) in &writes {
                ov.write(&pcell(*r), v.clone());
            }

            let result = ov.into_result();
            let keys: Vec<CellKey> = result.write_set_final.iter().map(|(k, _)| *k).collect();
            let unique: std::collections::BTreeSet<CellKey> = keys.iter().copied().collect();
            prop_assert_eq!(keys.len(), unique.len(), "duplicate keys in write_set_final");
        }
    }

    // --- Property 3: read_set_old has at most one entry per key ---
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
                let _ = ov.read(&pcell(*r)).unwrap();
            }

            let result = ov.into_result();
            let keys: Vec<CellKey> = result.read_set_old.iter().map(|(k, _)| *k).collect();
            let unique: std::collections::BTreeSet<CellKey> = keys.iter().copied().collect();
            prop_assert_eq!(keys.len(), unique.len(), "duplicate keys in read_set_old");
        }
    }

    // --- Property 4: Checkpoint-rollback preserves overlay state ---
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
                ov.write(&pcell(*r), v.clone());
            }

            let expected = ov.read(&pcell(read_row)).unwrap();

            ov.checkpoint();

            for (r, v) in &post_writes {
                ov.write(&pcell(*r), v.clone());
            }

            ov.rollback();

            let actual = ov.read(&pcell(read_row)).unwrap();
            prop_assert_eq!(actual, expected);
        }
    }

    // --- Property 5: Keys written before first read excluded from read_set_old ---
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
                ov.write(&pcell(*r), Value::U64(999));
            }
            for r in &read_rows {
                let _ = ov.read(&pcell(*r)).unwrap();
            }

            let result = ov.into_result();
            let read_keys: std::collections::BTreeSet<CellKey> =
                result.read_set_old.iter().map(|(k, _)| *k).collect();
            let write_keys: std::collections::BTreeSet<CellKey> =
                write_rows.iter().map(|r| pcell(*r)).collect();

            for k in &write_keys {
                prop_assert!(
                    !read_keys.contains(k),
                    "key {:?} was written before read but appears in read_set_old",
                    k
                );
            }
        }
    }

    // --- Property 6: Interpreter-produced events pass consistency checker ---
    proptest! {
        #[test]
        fn prop_interpreter_events_consistent(
            amounts in proptest::collection::vec(1u64..50, 1..5),
        ) {
            let initial_balance = 1000u64;
            let mut data = BTreeMap::new();
            data.insert(pcell(0), Value::U64(initial_balance));
            let snap = TestSnapshot(data);

            let mut prog = crate::program::Program::new();
            prog.register(TxTypeDef {
                id: TxTypeId(1),
                name: "withdraw".into(),
                param_schema: vec![ParamDef { name: "amount".into(), value_type: ValueType::U64 }],
                body: vec![
                    Instruction::Read {
                        dst: 0,
                        table: TableId(1),
                        row: RowExpr::Literal(RowKey(0)),
                        col: ColId(0),
                    },
                    Instruction::Sub {
                        dst: 1,
                        lhs: ValueExpr::Slot(0),
                        rhs: ValueExpr::Param(0),
                    },
                    Instruction::Write {
                        table: TableId(1),
                        row: RowExpr::Literal(RowKey(0)),
                        col: ColId(0),
                        src: ValueExpr::Slot(1),
                    },
                ],
            }).unwrap();

            let sender = [1u8; 32];
            let txs: Vec<Transaction> = amounts
                .iter()
                .enumerate()
                .map(|(i, &amt)| Transaction {
                    tx_type: TxTypeId(1),
                    params: vec![Value::U64(amt)],
                    sender,
                    nonce: i as u64,
                    signature: vec![],
                })
                .collect();
            let batch = Batch { transactions: txs };

            let env = crate::batch::BatchEnv {
                hasher: &XorHasher,
                sig_verifier: &AlwaysValidSig,
                nonce_policy: &SeqNonce,
                static_tables: &EmptyStaticTables,
            };
            let result = crate::batch::execute_batch(
                &batch, &prog, &snap, &env, &BTreeMap::new(),
            )
            .unwrap();

            let check = check_consistency(&result.events, &result.read_set_old);
            prop_assert!(check.is_ok(), "consistency check failed: {:?}", check.err());
        }
    }

    // --- Property 7: Tampered trace fails consistency checker ---
    proptest! {
        #[test]
        fn prop_tampered_trace_fails(
            tamper_idx in 0usize..5,
        ) {
            let k = pcell(0);
            let mut events = vec![
                ExecutionEvent { key: k, op: OpKind::Read, value: Value::U64(100), time: 0, tx_index: 0 },
                ExecutionEvent { key: k, op: OpKind::Write, value: Value::U64(80), time: 1, tx_index: 0 },
                ExecutionEvent { key: k, op: OpKind::Read, value: Value::U64(80), time: 2, tx_index: 0 },
                ExecutionEvent { key: k, op: OpKind::Write, value: Value::U64(60), time: 3, tx_index: 0 },
                ExecutionEvent { key: k, op: OpKind::Read, value: Value::U64(60), time: 4, tx_index: 0 },
            ];
            let read_set_old = vec![(k, Value::U64(100))];

            let idx = tamper_idx % events.len();
            match events[idx].op {
                OpKind::Read => {
                    events[idx].value = Value::U64(999);
                }
                OpKind::Write => {
                    events[idx].value = Value::U64(999);
                }
            }

            let check = check_consistency(&events, &read_set_old);
            prop_assert!(check.is_err(), "tampered trace should fail consistency check");
        }
    }
}
