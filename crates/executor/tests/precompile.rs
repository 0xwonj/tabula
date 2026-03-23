//! Precompile execution tests.

mod common;

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::{ColId, PortableValue, RowKey, TableId};
use tabula_executor::interpreter::ExecContext;
use tabula_executor::overlay::Overlay;
use tabula_executor::precompile::{PrecompileHandler, PrecompileRegistry};
use tabula_executor::property::PropertyQueryRegistry;
use tabula_ir::{
    Instruction, PrecompileId, PrecompileSignature, PrecompileValueProfile, RowExpr, ValueExpr,
};
use tabula_profile::{ENCODING_U64_ID, TYPE_U64_ID};
use tabula_types::{ArithmeticOp, TypedValue, u64_typed};

use common::*;

// ── Test precompile handlers ───────────────────────────────────────────

struct IdentityHandler;

fn u64_profile() -> PrecompileValueProfile {
    PrecompileValueProfile {
        type_id: TYPE_U64_ID,
        encoding_profile_id: ENCODING_U64_ID,
    }
}

impl PrecompileHandler for IdentityHandler {
    fn id(&self) -> PrecompileId {
        PrecompileId(0x0001)
    }

    fn signature(&self) -> &PrecompileSignature {
        static SIGNATURE: std::sync::OnceLock<PrecompileSignature> = std::sync::OnceLock::new();
        SIGNATURE.get_or_init(|| PrecompileSignature::new(vec![u64_profile()], vec![u64_profile()]))
    }

    fn execute(&self, inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError> {
        Ok(vec![inputs[0].clone()])
    }
}

struct AddHandler;

impl PrecompileHandler for AddHandler {
    fn id(&self) -> PrecompileId {
        PrecompileId(0x0002)
    }

    fn signature(&self) -> &PrecompileSignature {
        static SIGNATURE: std::sync::OnceLock<PrecompileSignature> = std::sync::OnceLock::new();
        SIGNATURE.get_or_init(|| {
            PrecompileSignature::new(vec![u64_profile(), u64_profile()], vec![u64_profile()])
        })
    }

    fn execute(&self, inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError> {
        let runtime = type_runtimes().resolve(inputs[0].type_id())?;
        runtime
            .apply_arithmetic(ArithmeticOp::Add, &inputs[0], &inputs[1])
            .map(|value| vec![value])
    }
}

struct SplitHandler;

impl PrecompileHandler for SplitHandler {
    fn id(&self) -> PrecompileId {
        PrecompileId(0x0003)
    }

    fn signature(&self) -> &PrecompileSignature {
        static SIGNATURE: std::sync::OnceLock<PrecompileSignature> = std::sync::OnceLock::new();
        SIGNATURE.get_or_init(|| {
            PrecompileSignature::new(vec![u64_profile()], vec![u64_profile(), u64_profile()])
        })
    }

    fn execute(&self, inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError> {
        let runtime = type_runtimes().resolve(inputs[0].type_id())?;
        let incremented = runtime.apply_arithmetic(ArithmeticOp::Add, &inputs[0], &u64_typed(1))?;
        Ok(vec![inputs[0].clone(), incremented])
    }
}

struct BadCountHandler;

impl PrecompileHandler for BadCountHandler {
    fn id(&self) -> PrecompileId {
        PrecompileId(0x0099)
    }

    fn signature(&self) -> &PrecompileSignature {
        static SIGNATURE: std::sync::OnceLock<PrecompileSignature> = std::sync::OnceLock::new();
        SIGNATURE.get_or_init(|| PrecompileSignature::new(vec![], vec![]))
    }

    fn execute(&self, _inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError> {
        Ok(vec![u64_typed(42)])
    }
}

struct BadInputTypeHandler;

impl PrecompileHandler for BadInputTypeHandler {
    fn id(&self) -> PrecompileId {
        PrecompileId(0x0100)
    }

    fn signature(&self) -> &PrecompileSignature {
        static SIGNATURE: std::sync::OnceLock<PrecompileSignature> = std::sync::OnceLock::new();
        SIGNATURE.get_or_init(|| {
            PrecompileSignature::new(
                vec![PrecompileValueProfile {
                    type_id: tabula_profile::TYPE_BOOL_ID,
                    encoding_profile_id: tabula_profile::ENCODING_BOOL_ID,
                }],
                vec![u64_profile()],
            )
        })
    }

    fn execute(&self, _inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError> {
        Ok(vec![u64_typed(1)])
    }
}

struct BadOutputTypeHandler;

impl PrecompileHandler for BadOutputTypeHandler {
    fn id(&self) -> PrecompileId {
        PrecompileId(0x0101)
    }

    fn signature(&self) -> &PrecompileSignature {
        static SIGNATURE: std::sync::OnceLock<PrecompileSignature> = std::sync::OnceLock::new();
        SIGNATURE.get_or_init(|| PrecompileSignature::new(vec![], vec![u64_profile()]))
    }

    fn execute(&self, _inputs: &[TypedValue]) -> Result<Vec<TypedValue>, TabulaError> {
        Ok(vec![tabula_types::bool_typed(true)])
    }
}

fn run_with_precompiles(
    instrs: &[Instruction],
    params: &[PortableValue],
    registry: &PrecompileRegistry,
) -> Result<tabula_executor::overlay::OverlayResult, tabula_executor::interpreter::InterpreterError>
{
    let snap = TestSnapshot(BTreeMap::new());
    let mut ov = Overlay::new(&snap, type_runtimes());
    let (schemas, profile_catalog) = test_schema_bundle();
    let property_queries = PropertyQueryRegistry::new();
    let ctx = ExecContext {
        hasher: &XorHasher,
        static_tables: &TestStaticTables,
        type_runtimes: type_runtimes(),
        schemas: &schemas,
        profile_catalog: &profile_catalog,
        precompiles: Some(registry),
        committed_state: None,
        property_queries: &property_queries,
    };
    let typed_params: Vec<_> = params.iter().cloned().map(typed).collect();
    tabula_executor::interpreter::execute(0, instrs, &typed_params, &mut ov, &ctx)?;
    Ok(ov.into_result().unwrap())
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
                inputs: vec![lit(u64_portable(42))],
            },
            Instruction::Write {
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(0)),
                src_val: ValueExpr::Slot(0),
                src_is_null: lit(bool_portable(false)),
            },
        ],
        &[],
        &reg,
    )
    .unwrap();

    assert_eq!(
        result.write_set_final,
        vec![(cell(1, 0, 0), opt(u64_portable(42)))]
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
                inputs: vec![lit(u64_portable(10)), lit(u64_portable(20))],
            },
            Instruction::Write {
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(0)),
                src_val: ValueExpr::Slot(0),
                src_is_null: lit(bool_portable(false)),
            },
        ],
        &[],
        &reg,
    )
    .unwrap();

    assert_eq!(
        result.write_set_final,
        vec![(cell(1, 0, 0), opt(u64_portable(30)))]
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
                inputs: vec![lit(u64_portable(100))],
            },
            Instruction::Write {
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(0)),
                src_val: ValueExpr::Slot(0),
                src_is_null: lit(bool_portable(false)),
            },
            Instruction::Write {
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(1)),
                src_val: ValueExpr::Slot(1),
                src_is_null: lit(bool_portable(false)),
            },
        ],
        &[],
        &reg,
    )
    .unwrap();

    assert_eq!(
        result.write_set_final,
        vec![
            (cell(1, 0, 0), opt(u64_portable(100))),
            (cell(1, 1, 0), opt(u64_portable(101))),
        ]
    );
}

#[test]
fn precompile_unknown_id_error() {
    let reg = PrecompileRegistry::new();

    let err = run_with_precompiles(
        &[Instruction::Precompile {
            id: PrecompileId(0xFFFF),
            dst_slots: vec![0],
            inputs: vec![lit(u64_portable(1))],
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
    reg.register(BadCountHandler).expect("register handler");

    let err = run_with_precompiles(
        &[Instruction::Precompile {
            id: PrecompileId(0x0099),
            dst_slots: vec![0, 1],
            inputs: vec![],
        }],
        &[],
        &reg,
    )
    .unwrap_err();

    assert!(
        err.error
            .to_string()
            .contains("returned 1 values but signature declares 0 outputs")
    );
}

#[test]
fn precompile_no_registry_error() {
    let err = run_err(vec![Instruction::Precompile {
        id: PrecompileId(0x0001),
        dst_slots: vec![0],
        inputs: vec![lit(u64_portable(1))],
    }]);

    assert!(
        err.error
            .to_string()
            .contains("no PrecompileRegistry provided")
    );
}

#[test]
fn precompile_input_signature_mismatch_fails_closed() {
    let mut reg = PrecompileRegistry::new();
    reg.register(BadInputTypeHandler).expect("register handler");

    let err = run_with_precompiles(
        &[Instruction::Precompile {
            id: PrecompileId(0x0100),
            dst_slots: vec![0],
            inputs: vec![lit(u64_portable(1))],
        }],
        &[],
        &reg,
    )
    .unwrap_err();

    assert!(err.error.to_string().contains("input 0 expects type"));
}

#[test]
fn precompile_output_type_mismatch_fails_closed() {
    let mut reg = PrecompileRegistry::new();
    reg.register(BadOutputTypeHandler)
        .expect("register handler");

    let err = run_with_precompiles(
        &[Instruction::Precompile {
            id: PrecompileId(0x0101),
            dst_slots: vec![0],
            inputs: vec![],
        }],
        &[],
        &reg,
    )
    .unwrap_err();

    assert!(err.error.to_string().contains("output 0 expects type"));
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
                src_is_null: lit(bool_portable(false)),
            },
        ],
        &[u64_portable(999)],
        &reg,
    )
    .unwrap();

    assert_eq!(
        result.write_set_final,
        vec![(cell(1, 0, 0), opt(u64_portable(999)))]
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
