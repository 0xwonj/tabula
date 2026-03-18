//! Precompile execution tests.

mod common;

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::{ColId, RowKey, TableId, Value};
use tabula_executor::interpreter::ExecContext;
use tabula_executor::overlay::Overlay;
use tabula_executor::precompile::{PrecompileHandler, PrecompileRegistry};
use tabula_executor::property::PropertyQueryRegistry;
use tabula_ir::{Instruction, PrecompileId, RowExpr, ValueExpr};

use common::*;

// ── Test precompile handlers ───────────────────────────────────────────

/// Identity: f(x) = x
struct IdentityHandler;
impl PrecompileHandler for IdentityHandler {
    fn id(&self) -> PrecompileId {
        PrecompileId(0x0001)
    }
    fn execute(&self, inputs: &[Value]) -> Result<Vec<Value>, TabulaError> {
        Ok(vec![inputs[0]])
    }
}

/// Add: f(a, b) = a + b
struct AddHandler;
impl PrecompileHandler for AddHandler {
    fn id(&self) -> PrecompileId {
        PrecompileId(0x0002)
    }
    fn execute(&self, inputs: &[Value]) -> Result<Vec<Value>, TabulaError> {
        inputs[0].checked_add(&inputs[1]).map(|v| vec![v])
    }
}

/// Split: f(a) = (a, a+1)
struct SplitHandler;
impl PrecompileHandler for SplitHandler {
    fn id(&self) -> PrecompileId {
        PrecompileId(0x0003)
    }
    fn execute(&self, inputs: &[Value]) -> Result<Vec<Value>, TabulaError> {
        let a = inputs[0];
        let b = a.checked_add(&Value::U64(1))?;
        Ok(vec![a, b])
    }
}

/// BadCount: always returns 1 value regardless of expected count.
struct BadCountHandler;
impl PrecompileHandler for BadCountHandler {
    fn id(&self) -> PrecompileId {
        PrecompileId(0x0099)
    }
    fn execute(&self, _inputs: &[Value]) -> Result<Vec<Value>, TabulaError> {
        Ok(vec![Value::U64(42)])
    }
}

fn run_with_precompiles(
    instrs: &[Instruction],
    params: &[Value],
    registry: &PrecompileRegistry,
) -> Result<tabula_executor::overlay::OverlayResult, tabula_executor::interpreter::InterpreterError>
{
    let snap = TestSnapshot(BTreeMap::new());
    let mut ov = Overlay::new(&snap);
    let schemas = test_schemas();
    let property_queries = PropertyQueryRegistry::new();
    let ctx = ExecContext {
        hasher: &XorHasher,
        static_tables: &TestStaticTables,
        schemas: &schemas,
        precompiles: Some(registry),
        committed_state: None,
        property_queries: &property_queries,
    };
    tabula_executor::interpreter::execute(instrs, params, &mut ov, &ctx)?;
    Ok(ov.into_result())
}

// ── Tests ──────────────────────────────────────────────────────────────

#[test]
fn precompile_identity_round_trip() {
    let mut reg = PrecompileRegistry::new();
    reg.register(IdentityHandler).expect("register handler");

    let result = run_with_precompiles(
        &[
            Instruction::Precompile {
                id: PrecompileId(0x0001),
                dst_slots: vec![0],
                inputs: vec![ValueExpr::Literal(Value::U64(42))],
            },
            Instruction::Write {
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(0)),
                src_val: ValueExpr::Slot(0),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
        ],
        &[],
        &reg,
    )
    .unwrap();

    assert_eq!(
        result.write_set_final,
        vec![(cell(1, 0, 0), Some(Value::U64(42)))]
    );
}

#[test]
fn precompile_multi_input() {
    let mut reg = PrecompileRegistry::new();
    reg.register(AddHandler).expect("register handler");

    let result = run_with_precompiles(
        &[
            Instruction::Precompile {
                id: PrecompileId(0x0002),
                dst_slots: vec![0],
                inputs: vec![
                    ValueExpr::Literal(Value::U64(10)),
                    ValueExpr::Literal(Value::U64(20)),
                ],
            },
            Instruction::Write {
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(0)),
                src_val: ValueExpr::Slot(0),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
        ],
        &[],
        &reg,
    )
    .unwrap();

    assert_eq!(
        result.write_set_final,
        vec![(cell(1, 0, 0), Some(Value::U64(30)))]
    );
}

#[test]
fn precompile_multi_output() {
    let mut reg = PrecompileRegistry::new();
    reg.register(SplitHandler).expect("register handler");

    let result = run_with_precompiles(
        &[
            Instruction::Precompile {
                id: PrecompileId(0x0003),
                dst_slots: vec![0, 1],
                inputs: vec![ValueExpr::Literal(Value::U64(100))],
            },
            // Write slot 0 (the original value)
            Instruction::Write {
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(0)),
                src_val: ValueExpr::Slot(0),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
            // Write slot 1 (the incremented value)
            Instruction::Write {
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(1)),
                src_val: ValueExpr::Slot(1),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
        ],
        &[],
        &reg,
    )
    .unwrap();

    // cell(t, r, c): both writes go to table=1, col=0, rows 0 and 1
    assert_eq!(
        result.write_set_final,
        vec![
            (cell(1, 0, 0), Some(Value::U64(100))),
            (cell(1, 1, 0), Some(Value::U64(101))),
        ]
    );
}

#[test]
fn precompile_unknown_id_error() {
    let reg = PrecompileRegistry::new(); // empty

    let err = run_with_precompiles(
        &[Instruction::Precompile {
            id: PrecompileId(0xFFFF),
            dst_slots: vec![0],
            inputs: vec![ValueExpr::Literal(Value::U64(1))],
        }],
        &[],
        &reg,
    )
    .unwrap_err();

    assert!(err.error.to_string().contains("unknown precompile ID"));
}

#[test]
fn precompile_wrong_result_count_error() {
    let mut reg = PrecompileRegistry::new();
    reg.register(BadCountHandler).expect("register handler"); // returns 1 value

    let err = run_with_precompiles(
        &[Instruction::Precompile {
            id: PrecompileId(0x0099),
            dst_slots: vec![0, 1], // expects 2 values
            inputs: vec![],
        }],
        &[],
        &reg,
    )
    .unwrap_err();

    assert!(err.error.to_string().contains("returned 1 values but 2"));
}

#[test]
fn precompile_no_registry_error() {
    // Use standard run() which has precompiles: None
    let err = run_err(vec![Instruction::Precompile {
        id: PrecompileId(0x0001),
        dst_slots: vec![0],
        inputs: vec![ValueExpr::Literal(Value::U64(1))],
    }]);

    assert!(
        err.error
            .to_string()
            .contains("no PrecompileRegistry provided")
    );
}

#[test]
fn precompile_with_param_input() {
    let mut reg = PrecompileRegistry::new();
    reg.register(IdentityHandler).expect("register handler");

    let result = run_with_precompiles(
        &[
            Instruction::Precompile {
                id: PrecompileId(0x0001),
                dst_slots: vec![0],
                inputs: vec![ValueExpr::Param(0)],
            },
            Instruction::Write {
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(0)),
                src_val: ValueExpr::Slot(0),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
        ],
        &[Value::U64(999)],
        &reg,
    )
    .unwrap();

    assert_eq!(
        result.write_set_final,
        vec![(cell(1, 0, 0), Some(Value::U64(999)))]
    );
}

#[test]
fn duplicate_precompile_registration_is_rejected() {
    let mut reg = PrecompileRegistry::new();
    reg.register(IdentityHandler)
        .expect("register first handler");

    let err = reg
        .register(IdentityHandler)
        .expect_err("duplicate registration should fail");

    assert!(err.to_string().contains("duplicate precompile ID"));
}
