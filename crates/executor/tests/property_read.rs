//! PropertyRead execution tests.

mod common;

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::{ColId, PortableValue, RowKey, TableId};
use tabula_executor::execute;
use tabula_executor::execute_tx;
use tabula_executor::interpreter::ExecContext;
use tabula_executor::overlay::Overlay;
use tabula_executor::property::{
    CommittedStateProvider, PropertyQueryHandler, PropertyQueryRegistry,
};
use tabula_ir::{Instruction, PropertyQuery, ValueExpr};
use tabula_types::{TypedColumnEntry, TypedPropertyQueryResult, TypedValue, bool_typed, u64_typed};

use common::*;

// ── Mock committed state ─────────────────────────────────────────────

#[allow(clippy::type_complexity)]
struct MockCommittedState {
    columns: BTreeMap<(TableId, ColId), Vec<(RowKey, PortableValue, bool)>>,
}

impl CommittedStateProvider for MockCommittedState {
    fn get_column(&self, table: TableId, col: ColId) -> Result<Vec<TypedColumnEntry>, TabulaError> {
        self.columns
            .get(&(table, col))
            .cloned()
            .map(|entries| {
                entries
                    .into_iter()
                    .map(|(row_key, value, is_null)| TypedColumnEntry {
                        row_key,
                        value: typed(value),
                        is_null,
                    })
                    .collect()
            })
            .ok_or(TabulaError::TableNotFound(table))
    }
}

// ── Mock property resolver ───────────────────────────────────────────

/// Simple resolver that handles Minimum queries by finding the entry with
/// the smallest key.
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
                let non_null: Vec<_> = entries.iter().filter(|entry| !entry.is_null).collect();
                if non_null.is_empty() {
                    Ok(TypedPropertyQueryResult {
                        value: typed(u64_portable(0)),
                        key: None,
                        is_null: true,
                    })
                } else {
                    let min_entry = non_null.iter().min_by_key(|entry| entry.row_key.0).unwrap();
                    Ok(TypedPropertyQueryResult {
                        value: min_entry.value.clone(),
                        key: Some(min_entry.row_key),
                        is_null: false,
                    })
                }
            }
            PropertyQuery::Maximum => {
                let non_null: Vec<_> = entries.iter().filter(|entry| !entry.is_null).collect();
                if non_null.is_empty() {
                    Ok(TypedPropertyQueryResult {
                        value: typed(u64_portable(0)),
                        key: None,
                        is_null: true,
                    })
                } else {
                    let max_entry = non_null.iter().max_by_key(|entry| entry.row_key.0).unwrap();
                    Ok(TypedPropertyQueryResult {
                        value: max_entry.value.clone(),
                        key: Some(max_entry.row_key),
                        is_null: false,
                    })
                }
            }
            _ => Err(TabulaError::InvalidIr(format!(
                "unsupported property query: {query:?}"
            ))),
        }
    }
}

// ── Helper ───────────────────────────────────────────────────────────

/// Execute property read instructions and return slot values via Write pattern.
fn execute_and_get_slots(
    instrs: &[Instruction],
    committed: &MockCommittedState,
    registry: &PropertyQueryRegistry,
    num_slots: usize,
) -> Vec<TypedValue> {
    let snap = TestSnapshot(BTreeMap::new());
    let mut ov = Overlay::new(&snap, type_runtimes());
    let execution_program = test_execution_program();
    let ctx = ExecContext {
        hasher: &XorHasher,
        static_tables: &TestStaticTables,
        type_runtimes: type_runtimes(),
        execution_program: &execution_program,
        precompiles: None,
        committed_state: Some(committed),
        property_queries: registry,
    };
    let mut full = instrs.to_vec();
    for i in 0..num_slots {
        full.push(Instruction::Write {
            table: TableId(1),
            col: ColId(0),
            row: tabula_ir::RowExpr::Literal(RowKey(1000 + i as u64)),
            src_val: ValueExpr::Slot(i as u16),
            src_is_null: lit(bool_portable(false)),
        });
    }
    execute_tx(0, &full, &[], &mut ov, &ctx).unwrap();
    let result = ov.into_result().unwrap();
    let portable_writes = portable_write_set(&result);

    (0..num_slots)
        .map(|i| {
            let key = tabula_core::CellKey {
                table: TableId(1),
                col: ColId(0),
                row: RowKey(1000 + i as u64),
            };
            portable_writes
                .iter()
                .find(|(k, _)| *k == key)
                .and_then(|(_, v)| v.clone())
                .map_or(u64_typed(0), |value| {
                    type_runtimes()
                        .resolve(value.type_id())
                        .expect("runtime type for test slot")
                        .decode_portable(&value)
                        .expect("typed built-in value for test assertion")
                })
        })
        .collect()
}

// ── Tests ────────────────────────────────────────────────────────────

#[test]
fn property_read_minimum() {
    let mut columns = BTreeMap::new();
    columns.insert(
        (TableId(1), ColId(0)),
        vec![
            (RowKey(10), u64_portable(100), false),
            (RowKey(5), u64_portable(50), false),
            (RowKey(20), u64_portable(200), false),
        ],
    );
    let committed = MockCommittedState { columns };
    let mut registry = PropertyQueryRegistry::new();
    registry
        .register(TableId(1), ColId(0), Box::new(MinimumResolver))
        .expect("register property handler");

    let instrs = vec![Instruction::PropertyRead {
        dst_val: 0,
        dst_key: 1,
        dst_is_null: 2,
        table: TableId(1),
        col: ColId(0),
        query: PropertyQuery::Minimum,
    }];

    let slots = execute_and_get_slots(&instrs, &committed, &registry, 3);
    assert_eq!(slots[0], u64_typed(50));
    assert_eq!(slots[1], u64_typed(5));
    assert_eq!(slots[2], bool_typed(false));
}

#[test]
fn property_read_minimum_empty_column() {
    let mut columns = BTreeMap::new();
    columns.insert((TableId(1), ColId(0)), vec![]);
    let committed = MockCommittedState { columns };
    let mut registry = PropertyQueryRegistry::new();
    registry
        .register(TableId(1), ColId(0), Box::new(MinimumResolver))
        .expect("register property handler");

    let instrs = vec![Instruction::PropertyRead {
        dst_val: 0,
        dst_key: 1,
        dst_is_null: 2,
        table: TableId(1),
        col: ColId(0),
        query: PropertyQuery::Minimum,
    }];

    let slots = execute_and_get_slots(&instrs, &committed, &registry, 3);
    assert_eq!(slots[2], bool_typed(true));
}

#[test]
fn property_read_maximum() {
    let mut columns = BTreeMap::new();
    columns.insert(
        (TableId(1), ColId(0)),
        vec![
            (RowKey(10), u64_portable(100), false),
            (RowKey(5), u64_portable(50), false),
            (RowKey(20), u64_portable(200), false),
        ],
    );
    let committed = MockCommittedState { columns };
    let mut registry = PropertyQueryRegistry::new();
    registry
        .register(TableId(1), ColId(0), Box::new(MinimumResolver))
        .expect("register property handler");

    let instrs = vec![Instruction::PropertyRead {
        dst_val: 0,
        dst_key: 1,
        dst_is_null: 2,
        table: TableId(1),
        col: ColId(0),
        query: PropertyQuery::Maximum,
    }];

    let slots = execute_and_get_slots(&instrs, &committed, &registry, 3);
    assert_eq!(slots[0], u64_typed(200));
    assert_eq!(slots[1], u64_typed(20));
    assert_eq!(slots[2], bool_typed(false));
}

#[test]
fn property_read_no_provider_error() {
    let snap = TestSnapshot(BTreeMap::new());
    let mut ov = Overlay::new(&snap, type_runtimes());
    let execution_program = test_execution_program();
    let property_queries = PropertyQueryRegistry::new();
    let ctx = ExecContext {
        hasher: &XorHasher,
        static_tables: &TestStaticTables,
        type_runtimes: type_runtimes(),
        execution_program: &execution_program,
        precompiles: None,
        committed_state: None,
        property_queries: &property_queries,
    };
    let instrs = vec![Instruction::PropertyRead {
        dst_val: 0,
        dst_key: 1,
        dst_is_null: 2,
        table: TableId(1),
        col: ColId(0),
        query: PropertyQuery::Minimum,
    }];
    let err = execute(0, &instrs, &[], &mut ov, &ctx).unwrap_err();
    assert!(err.error.to_string().contains("CommittedStateProvider"));
}

#[test]
fn property_read_result_usable_in_assert() {
    let mut columns = BTreeMap::new();
    columns.insert(
        (TableId(1), ColId(0)),
        vec![(RowKey(42), u64_portable(999), false)],
    );
    let committed = MockCommittedState { columns };
    let mut registry = PropertyQueryRegistry::new();
    registry
        .register(TableId(1), ColId(0), Box::new(MinimumResolver))
        .expect("register property handler");

    let snap = TestSnapshot(BTreeMap::new());
    let mut ov = Overlay::new(&snap, type_runtimes());
    let execution_program = test_execution_program();
    let ctx = ExecContext {
        hasher: &XorHasher,
        static_tables: &TestStaticTables,
        type_runtimes: type_runtimes(),
        execution_program: &execution_program,
        precompiles: None,
        committed_state: Some(&committed),
        property_queries: &registry,
    };

    let instrs = vec![
        Instruction::PropertyRead {
            dst_val: 0,
            dst_key: 1,
            dst_is_null: 2,
            table: TableId(1),
            col: ColId(0),
            query: PropertyQuery::Minimum,
        },
        Instruction::Cmp {
            dst: 3,
            op: tabula_ir::CmpOp::Eq,
            lhs: ValueExpr::Slot(0),
            rhs: lit(u64_portable(999)),
        },
        Instruction::Assert {
            cond: ValueExpr::Slot(3),
        },
        Instruction::Cmp {
            dst: 4,
            op: tabula_ir::CmpOp::Eq,
            lhs: ValueExpr::Slot(1),
            rhs: lit(u64_portable(42)),
        },
        Instruction::Assert {
            cond: ValueExpr::Slot(4),
        },
    ];

    execute(0, &instrs, &[], &mut ov, &ctx).unwrap();
}
