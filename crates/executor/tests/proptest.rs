//! Property-based tests for overlay semantics and consistency.

mod common;

use std::collections::BTreeMap;

use proptest::prelude::*;

use tabula_core::{Batch, ColId, OpKind, PortableValue, RowKey, TableId, Transaction, TxTypeId};
use tabula_ir::{ArithOp, Instruction, ParamDef, RowExpr, TxTypeDef};
use tabula_profile::TYPE_U64_ID;
use tabula_types::u64_portable;

use tabula_executor::consistency::{check_consistency, check_journal_consistency};
use tabula_executor::overlay::Overlay;
use tabula_executor::property::PropertyQueryRegistry;
use tabula_executor::{ResolvedExecutionProgram, execute_batch};

use common::*;

fn pcell(r: u64) -> tabula_core::CellKey {
    cell(1, r, 0)
}

fn arb_value() -> impl Strategy<Value = Option<PortableValue>> {
    prop_oneof![any::<u64>().prop_map(|n| Some(u64_portable(n))), Just(None)]
}

fn arb_row() -> impl Strategy<Value = u64> {
    0u64..10
}

proptest! {
    #[test]
    fn prop_read_after_write_returns_last(
        writes in proptest::collection::vec((arb_row(), arb_value()), 1..20),
        read_row in arb_row(),
    ) {
        let snap = TestSnapshot(BTreeMap::new());
        let mut ov = Overlay::new(&snap, type_runtimes());

        for (r, v) in &writes {
            ov.write(&pcell(*r), v.clone().map(typed), TYPE_U64_ID).unwrap();
        }

        let result = ov.read(&pcell(read_row), TYPE_U64_ID).unwrap();
        let expected: Option<_> = writes
            .iter()
            .rev()
            .find(|(r, _)| *r == read_row)
            .and_then(|(_, v)| v.clone().map(typed));

        prop_assert_eq!(result, expected);
    }
}

proptest! {
    #[test]
    fn prop_write_set_one_per_key(
        writes in proptest::collection::vec((arb_row(), arb_value()), 1..30),
    ) {
        let snap = TestSnapshot(BTreeMap::new());
        let mut ov = Overlay::new(&snap, type_runtimes());

        for (r, v) in &writes {
            ov.write(&pcell(*r), v.clone().map(typed), TYPE_U64_ID).unwrap();
        }

        let result = ov.into_result().unwrap();
        let keys: Vec<_> = result.write_set_final.iter().map(|entry| entry.key).collect();
        let unique: std::collections::BTreeSet<_> = keys.iter().copied().collect();
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
            data.insert(pcell(r), u64_portable(r * 10));
        }
        let snap = snapshot(data);
        let mut ov = Overlay::new(&snap, type_runtimes());

        for r in &reads {
            let _ = ov.read(&pcell(*r), TYPE_U64_ID).unwrap();
        }

        let result = ov.into_result().unwrap();
        let keys: Vec<_> = result.read_set_old.iter().map(|entry| entry.key).collect();
        let unique: std::collections::BTreeSet<_> = keys.iter().copied().collect();
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
        let mut ov = Overlay::new(&snap, type_runtimes());

        for (r, v) in &pre_writes {
            ov.write(&pcell(*r), v.clone().map(typed), TYPE_U64_ID).unwrap();
        }

        let expected = ov.read(&pcell(read_row), TYPE_U64_ID).unwrap();

        ov.checkpoint();
        for (r, v) in &post_writes {
            ov.write(&pcell(*r), v.clone().map(typed), TYPE_U64_ID).unwrap();
        }

        ov.rollback();

        let actual = ov.read(&pcell(read_row), TYPE_U64_ID).unwrap();
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
            data.insert(pcell(r), u64_portable(r));
        }
        let snap = snapshot(data);
        let mut ov = Overlay::new(&snap, type_runtimes());

        for r in &write_rows {
            ov.write(&pcell(*r), Some(typed(u64_portable(999))), TYPE_U64_ID).unwrap();
        }
        for r in &read_rows {
            let _ = ov.read(&pcell(*r), TYPE_U64_ID).unwrap();
        }

        let result = ov.into_result().unwrap();
        let read_keys: std::collections::BTreeSet<_> =
            result.read_set_old.iter().map(|entry| entry.key).collect();
        let write_keys: std::collections::BTreeSet<_> =
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
        data.insert(pcell(0), u64_portable(initial_balance));
        let snap = snapshot(data);

        let (schemas, profile_catalog) = test_schema_bundle();
        let mut prog = tabula_ir::Program::with_profile_catalog(profile_catalog);
        prog.add_schema(schemas.get(&TableId(1)).expect("test schema").clone());
        prog.register(TxTypeDef {
            id: TxTypeId(1),
            name: "withdraw".into(),
            param_schema: vec![ParamDef {
                name: "amount".into(),
                type_id: TYPE_U64_ID,
            }],
            body: vec![
                Instruction::Read {
                    dst_val: 0,
                    dst_is_null: 1,
                    table: TableId(1),
                    row: RowExpr::Literal(RowKey(0)),
                    col: ColId(0),
                },
                Instruction::Arith {
                    dst: 2,
                    op: ArithOp::Sub,
                    lhs: tabula_ir::ValueExpr::Slot(0),
                    rhs: tabula_ir::ValueExpr::Param(0),
                },
                Instruction::Write {
                    table: TableId(1),
                    row: RowExpr::Literal(RowKey(0)),
                    col: ColId(0),
                    src_val: tabula_ir::ValueExpr::Slot(2),
                    src_is_null: lit(bool_portable(false)),
                },
            ],
        }).unwrap();

        let sender = [1u8; 32];
        let txs: Vec<Transaction> = amounts.iter().enumerate()
            .map(|(i, &amt)| Transaction {
                tx_type: TxTypeId(1),
                params: vec![u64_portable(amt)],
                sender,
                nonce: i as u64,
                signature: vec![],
            })
            .collect();
        let batch = Batch { transactions: txs };

        let property_queries = PropertyQueryRegistry::new();
        let env = tabula_executor::batch::BatchEnv {
            hasher: &XorHasher,
            sig_verifier: &AlwaysValidSig,
            nonce_policy: &SeqNonce,
            static_tables: &EmptyStaticTables,
            precompiles: None,
            committed_state: None,
            property_queries: &property_queries,
            type_runtimes: type_runtimes(),
        };
        let resolved = ResolvedExecutionProgram::from_program(&prog).unwrap();
        let result = execute_batch(&batch, &resolved, &snap, &env, &BTreeMap::new()).unwrap();
        let check = check_journal_consistency(&result);
        prop_assert!(check.is_ok(), "consistency check failed: {:?}", check.err());
    }
}

proptest! {
    #[test]
    fn prop_tampered_trace_fails(
        tamper_idx in 0usize..5,
    ) {
        use tabula_core::{AccessEvent, TxResult};

        let k = pcell(0);
        let mut events = vec![
            AccessEvent { key: k, op: OpKind::Read, value: portable(u64_portable(100)), val_is_null: false, time: 0, effect_ordinal_in_tx: 0 },
            AccessEvent { key: k, op: OpKind::Write, value: portable(u64_portable(80)), val_is_null: false, time: 1, effect_ordinal_in_tx: 1 },
            AccessEvent { key: k, op: OpKind::Read, value: portable(u64_portable(80)), val_is_null: false, time: 2, effect_ordinal_in_tx: 2 },
            AccessEvent { key: k, op: OpKind::Write, value: portable(u64_portable(60)), val_is_null: false, time: 3, effect_ordinal_in_tx: 3 },
            AccessEvent { key: k, op: OpKind::Read, value: portable(u64_portable(60)), val_is_null: false, time: 4, effect_ordinal_in_tx: 4 },
        ];
        let read_set_old = vec![(k, opt(u64_portable(100)))];

        let idx = tamper_idx % events.len();
        events[idx].value = portable(u64_portable(999));

        let txs = vec![TxResult::success(events.clone(), vec![])];
        let check = check_consistency(&events, &read_set_old, &txs);
        prop_assert!(check.is_err(), "tampered trace should fail consistency check");
    }
}
