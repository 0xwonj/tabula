//! PropertyRead execution tests.

mod common;

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::{ColId, ColumnDef, RowKey, TableId, TableSchema, Value, ValueType};
use tabula_executor::interpreter::{ExecContext, execute};
use tabula_executor::overlay::Overlay;
use tabula_executor::property::{
    CommittedStateProvider, PropertyOpeningRegistry, PropertyOpeningResolver, PropertyResult,
};
use tabula_ir::{Instruction, PropertyQuery, ValueExpr};

use common::*;

// ── Mock committed state ─────────────────────────────────────────────

#[allow(clippy::type_complexity)]
struct MockCommittedState {
    columns: BTreeMap<(TableId, ColId), Vec<(RowKey, Value, bool)>>,
}

impl CommittedStateProvider for MockCommittedState {
    fn get_column(
        &self,
        table: TableId,
        col: ColId,
    ) -> Result<Vec<(RowKey, Value, bool)>, TabulaError> {
        self.columns
            .get(&(table, col))
            .cloned()
            .ok_or(TabulaError::TableNotFound(table))
    }
}

// ── Mock property resolver ───────────────────────────────────────────

/// Simple resolver that handles Minimum queries by finding the entry with
/// the smallest key.
struct MinimumResolver;

impl PropertyOpeningResolver for MinimumResolver {
    fn resolve(
        &self,
        table: TableId,
        col: ColId,
        query: &PropertyQuery,
        provider: &dyn CommittedStateProvider,
        _col_type: ValueType,
    ) -> Result<PropertyResult, TabulaError> {
        let entries = provider.get_column(table, col)?;

        match query {
            PropertyQuery::Minimum => {
                let non_null: Vec<_> = entries.iter().filter(|(_, _, null)| !null).collect();
                if non_null.is_empty() {
                    Ok(PropertyResult {
                        value: Value::U64(0),
                        key: None,
                        is_null: true,
                    })
                } else {
                    // Minimum key among non-null entries.
                    let min_entry = non_null.iter().min_by_key(|(k, _, _)| k.0).unwrap();
                    Ok(PropertyResult {
                        value: min_entry.1,
                        key: Some(min_entry.0),
                        is_null: false,
                    })
                }
            }
            PropertyQuery::Maximum => {
                let non_null: Vec<_> = entries.iter().filter(|(_, _, null)| !null).collect();
                if non_null.is_empty() {
                    Ok(PropertyResult {
                        value: Value::U64(0),
                        key: None,
                        is_null: true,
                    })
                } else {
                    let max_entry = non_null.iter().max_by_key(|(k, _, _)| k.0).unwrap();
                    Ok(PropertyResult {
                        value: max_entry.1,
                        key: Some(max_entry.0),
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

fn schemas_with_table() -> BTreeMap<TableId, TableSchema> {
    let mut m = BTreeMap::new();
    m.insert(
        TableId(1),
        TableSchema {
            id: TableId(1),
            name: "t".into(),
            columns: vec![ColumnDef {
                id: ColId(0),
                name: "val".into(),
                value_type: ValueType::U64,
            }],
        },
    );
    m
}

/// Execute property read instructions and return slot values via Write pattern.
fn execute_and_get_slots(
    instrs: &[Instruction],
    committed: &MockCommittedState,
    registry: &PropertyOpeningRegistry,
    num_slots: usize,
) -> Vec<Value> {
    let snap = TestSnapshot(BTreeMap::new());
    let mut ov = Overlay::new(&snap);
    let schemas = schemas_with_table();
    let ctx = ExecContext {
        hasher: &XorHasher,
        static_tables: &TestStaticTables,
        schemas: &schemas,
        precompiles: None,
        committed_state: Some(committed),
        property_openings: Some(registry),
    };
    // Build extended instructions: original + writes for each slot to different rows.
    let mut full = instrs.to_vec();
    for i in 0..num_slots {
        full.push(Instruction::Write {
            table: TableId(1),
            col: ColId(0),
            row: tabula_ir::RowExpr::Literal(RowKey(1000 + i as u64)),
            src_val: ValueExpr::Slot(i as u16),
            src_is_null: ValueExpr::Literal(Value::Bool(false)),
        });
    }
    execute(&full, &[], &mut ov, &ctx).unwrap();
    let result = ov.into_result();

    // Read the values back from writes.
    (0..num_slots)
        .map(|i| {
            let key = tabula_core::CellKey {
                table: TableId(1),
                col: ColId(0),
                row: RowKey(1000 + i as u64),
            };
            result
                .write_set_final
                .iter()
                .find(|(k, _)| *k == key)
                .and_then(|(_, v)| *v)
                .unwrap_or(Value::U64(0))
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
            (RowKey(10), Value::U64(100), false),
            (RowKey(5), Value::U64(50), false),
            (RowKey(20), Value::U64(200), false),
        ],
    );
    let committed = MockCommittedState { columns };
    let registry = PropertyOpeningRegistry::new(Box::new(MinimumResolver));

    let instrs = vec![Instruction::PropertyRead {
        dst_val: 0,
        dst_key: 1,
        dst_is_null: 2,
        table: TableId(1),
        col: ColId(0),
        query: PropertyQuery::Minimum,
    }];

    let slots = execute_and_get_slots(&instrs, &committed, &registry, 3);
    assert_eq!(slots[0], Value::U64(50)); // minimum value (at key 5)
    assert_eq!(slots[1], Value::U64(5)); // minimum key
    assert_eq!(slots[2], Value::Bool(false)); // not null
}

#[test]
fn property_read_minimum_empty_column() {
    let mut columns = BTreeMap::new();
    columns.insert((TableId(1), ColId(0)), vec![]);
    let committed = MockCommittedState { columns };
    let registry = PropertyOpeningRegistry::new(Box::new(MinimumResolver));

    let instrs = vec![Instruction::PropertyRead {
        dst_val: 0,
        dst_key: 1,
        dst_is_null: 2,
        table: TableId(1),
        col: ColId(0),
        query: PropertyQuery::Minimum,
    }];

    let slots = execute_and_get_slots(&instrs, &committed, &registry, 3);
    assert_eq!(slots[2], Value::Bool(true)); // is_null = true
}

#[test]
fn property_read_maximum() {
    let mut columns = BTreeMap::new();
    columns.insert(
        (TableId(1), ColId(0)),
        vec![
            (RowKey(10), Value::U64(100), false),
            (RowKey(5), Value::U64(50), false),
            (RowKey(20), Value::U64(200), false),
        ],
    );
    let committed = MockCommittedState { columns };
    let registry = PropertyOpeningRegistry::new(Box::new(MinimumResolver));

    let instrs = vec![Instruction::PropertyRead {
        dst_val: 0,
        dst_key: 1,
        dst_is_null: 2,
        table: TableId(1),
        col: ColId(0),
        query: PropertyQuery::Maximum,
    }];

    let slots = execute_and_get_slots(&instrs, &committed, &registry, 3);
    assert_eq!(slots[0], Value::U64(200)); // max value (at key 20)
    assert_eq!(slots[1], Value::U64(20)); // max key
    assert_eq!(slots[2], Value::Bool(false)); // not null
}

#[test]
fn property_read_no_provider_error() {
    let snap = TestSnapshot(BTreeMap::new());
    let mut ov = Overlay::new(&snap);
    let schemas = schemas_with_table();
    let ctx = ExecContext {
        hasher: &XorHasher,
        static_tables: &TestStaticTables,
        schemas: &schemas,
        precompiles: None,
        committed_state: None, // no provider
        property_openings: None,
    };
    let instrs = vec![Instruction::PropertyRead {
        dst_val: 0,
        dst_key: 1,
        dst_is_null: 2,
        table: TableId(1),
        col: ColId(0),
        query: PropertyQuery::Minimum,
    }];
    let err = execute(&instrs, &[], &mut ov, &ctx).unwrap_err();
    assert!(err.error.to_string().contains("CommittedStateProvider"));
}

#[test]
fn property_read_result_usable_in_assert() {
    let mut columns = BTreeMap::new();
    columns.insert(
        (TableId(1), ColId(0)),
        vec![(RowKey(42), Value::U64(999), false)],
    );
    let committed = MockCommittedState { columns };
    let registry = PropertyOpeningRegistry::new(Box::new(MinimumResolver));

    let snap = TestSnapshot(BTreeMap::new());
    let mut ov = Overlay::new(&snap);
    let schemas = schemas_with_table();
    let ctx = ExecContext {
        hasher: &XorHasher,
        static_tables: &TestStaticTables,
        schemas: &schemas,
        precompiles: None,
        committed_state: Some(&committed),
        property_openings: Some(&registry),
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
        // Assert the value equals 999
        Instruction::Cmp {
            dst: 3,
            op: tabula_ir::CmpOp::Eq,
            lhs: ValueExpr::Slot(0),
            rhs: ValueExpr::Literal(Value::U64(999)),
        },
        Instruction::Assert {
            cond: ValueExpr::Slot(3),
        },
        // Assert key equals 42
        Instruction::Cmp {
            dst: 4,
            op: tabula_ir::CmpOp::Eq,
            lhs: ValueExpr::Slot(1),
            rhs: ValueExpr::Literal(Value::U64(42)),
        },
        Instruction::Assert {
            cond: ValueExpr::Slot(4),
        },
    ];

    execute(&instrs, &[], &mut ov, &ctx).unwrap();
}
