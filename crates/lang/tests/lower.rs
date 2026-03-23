#![allow(missing_docs)]
use tabula_core::{
    ColId, ColumnProfileId, PortableValue, SchemeId, TableId, TableSchema, TxTypeId, TypeId,
};
use tabula_ir::{
    ArithOp, CmpOp, Instruction, PrecompileId, PrecompileSignature, PrecompileValueProfile,
    Program, RowExpr, ValueExpr,
};
use tabula_lang::error::ErrorKind;
use tabula_lang::lexer::lex;
use tabula_lang::lower::{
    LoweredProgram, lower, lower_with_registry, lower_with_registry_and_precompiles,
};
use tabula_lang::parser::parse;
use tabula_profile::{
    ColumnProfile, CommitmentRole, ENCODING_BOOL_ID, ENCODING_U64_ID, GenericIrFamily,
    HostValueFamily, NullSemantics, SCHEME_PROFILE_SSMC_ID, SemanticRegistry, TYPE_BOOL_ID,
    TYPE_U64_ID, TypeCapabilities, TypeDescriptor, ZeroValueSpec, builtin_catalog,
    builtin_semantic_registry,
};

const CUSTOM_SOURCE_NUMERIC_TYPE_ID: TypeId = TypeId(0xc101);
const CUSTOM_SOURCE_OPAQUE_ZERO_TYPE_ID: TypeId = TypeId(0xc102);
const CUSTOM_SOURCE_SMALL_INT_TYPE_ID: TypeId = TypeId(0xc103);

fn compile(source: &str) -> LoweredProgram {
    let tokens = lex(source).expect("lex failed");
    let ast = parse(tokens).expect("parse failed");
    lower(&ast).expect("lower failed")
}

fn compile_with_precompiles(
    source: &str,
    precompiles: &[(PrecompileId, PrecompileSignature)],
) -> LoweredProgram {
    let tokens = lex(source).expect("lex failed");
    let ast = parse(tokens).expect("parse failed");
    let precompiles = precompiles.iter().cloned().collect();
    let registry = builtin_semantic_registry().expect("built-in semantic registry");
    lower_with_registry_and_precompiles(&ast, &registry, &precompiles).expect("lower failed")
}

fn compile_with_registry_for_test(source: &str, registry: &SemanticRegistry) -> LoweredProgram {
    let tokens = lex(source).expect("lex failed");
    let ast = parse(tokens).expect("parse failed");
    lower_with_registry(&ast, registry).expect("lower failed")
}

fn compile_with_registry_err(
    source: &str,
    registry: &SemanticRegistry,
) -> Vec<tabula_lang::error::CompileError> {
    let tokens = lex(source).expect("lex failed");
    let ast = parse(tokens).expect("parse failed");
    lower_with_registry(&ast, registry).expect_err("lower should fail")
}

fn profile(
    type_id: tabula_core::TypeId,
    encoding_profile_id: tabula_core::EncodingProfileId,
) -> PrecompileValueProfile {
    PrecompileValueProfile {
        type_id,
        encoding_profile_id,
    }
}

fn portable_u64(value: u64) -> PortableValue {
    PortableValue::new(TYPE_U64_ID, value.to_le_bytes().to_vec())
}

fn portable_bool(value: bool) -> PortableValue {
    PortableValue::new(TYPE_BOOL_ID, vec![u8::from(value)])
}

fn portable_zero(type_id: TypeId) -> PortableValue {
    PortableValue::new(type_id, 0u64.to_le_bytes().to_vec())
}

fn custom_numeric_registry() -> SemanticRegistry {
    let mut registry = SemanticRegistry::new();
    registry
        .register_type_descriptor(
            TypeDescriptor::new(
                CUSTOM_SOURCE_NUMERIC_TYPE_ID,
                "u64",
                None,
                HostValueFamily::UnsignedInt { bits: 64 },
                GenericIrFamily::UnsignedInteger,
                TypeCapabilities {
                    equality: true,
                    ordering: true,
                    arithmetic: true,
                },
                ZeroValueSpec::IntegerZero,
                NullSemantics::NullableWithCanonicalZero,
            )
            .expect("numeric descriptor"),
        )
        .expect("register numeric descriptor");
    registry
        .register_type_name("u64", CUSTOM_SOURCE_NUMERIC_TYPE_ID)
        .expect("register numeric name");
    registry
}

fn opaque_zero_registry() -> SemanticRegistry {
    let mut registry = SemanticRegistry::new();
    registry
        .register_type_descriptor(
            TypeDescriptor::new(
                CUSTOM_SOURCE_OPAQUE_ZERO_TYPE_ID,
                "u64",
                None,
                HostValueFamily::Opaque {
                    family: "opaque_u64".to_string(),
                },
                GenericIrFamily::EqOnly,
                TypeCapabilities {
                    equality: true,
                    ordering: false,
                    arithmetic: false,
                },
                ZeroValueSpec::Opaque {
                    label: "opaque_zero".to_string(),
                },
                NullSemantics::NullableWithCanonicalZero,
            )
            .expect("opaque descriptor"),
        )
        .expect("register opaque descriptor");
    registry
        .register_type_name("u64", CUSTOM_SOURCE_OPAQUE_ZERO_TYPE_ID)
        .expect("register opaque name");
    registry
}

fn small_int_registry() -> SemanticRegistry {
    let mut registry = SemanticRegistry::new();
    registry
        .register_type_descriptor(
            TypeDescriptor::new(
                CUSTOM_SOURCE_SMALL_INT_TYPE_ID,
                "u64",
                None,
                HostValueFamily::UnsignedInt { bits: 32 },
                GenericIrFamily::UnsignedInteger,
                TypeCapabilities {
                    equality: true,
                    ordering: true,
                    arithmetic: true,
                },
                ZeroValueSpec::IntegerZero,
                NullSemantics::NullableWithCanonicalZero,
            )
            .expect("small-int descriptor"),
        )
        .expect("register small-int descriptor");
    registry
        .register_type_name("u64", CUSTOM_SOURCE_SMALL_INT_TYPE_ID)
        .expect("register small-int name");
    registry
}

fn lit_bool(value: bool) -> ValueExpr {
    ValueExpr::Literal(portable_bool(value))
}

fn seal_schemas(compiled: &LoweredProgram) -> (Vec<TableSchema>, tabula_profile::ProfileCatalog) {
    let mut catalog = builtin_catalog().expect("built-in catalog");
    let mut next_column_profile_id = 0u32;
    let schemas = compiled
        .schemas
        .iter()
        .map(|schema| {
            let columns = schema
                .columns
                .iter()
                .map(|column| {
                    let type_descriptor = catalog
                        .types
                        .iter()
                        .find(|descriptor| descriptor.type_id == column.type_id)
                        .cloned()
                        .expect("type descriptor");
                    let encoding_id = match column.type_id {
                        TYPE_U64_ID => ENCODING_U64_ID,
                        TYPE_BOOL_ID => ENCODING_BOOL_ID,
                        other => panic!("unsupported built-in type id in lang tests: {}", other.0),
                    };
                    let encoding_profile = catalog
                        .encodings
                        .iter()
                        .find(|profile| profile.encoding_profile_id == encoding_id)
                        .cloned()
                        .expect("encoding profile");
                    let scheme_profile = catalog
                        .schemes
                        .iter()
                        .find(|profile| profile.scheme_profile_id == SCHEME_PROFILE_SSMC_ID)
                        .cloned()
                        .expect("ssmc scheme profile");
                    let column_profile = ColumnProfile::new(
                        ColumnProfileId(next_column_profile_id),
                        format!("{}.{}", schema.name, column.name),
                        None,
                        &type_descriptor,
                        &encoding_profile,
                        &scheme_profile,
                        CommitmentRole::IncludedInRoot,
                    )
                    .expect("column profile");
                    let column_profile_id = column_profile.column_profile_id;
                    catalog
                        .register_column(column_profile)
                        .expect("register column");
                    next_column_profile_id += 1;
                    tabula_core::ColumnDef {
                        id: column.id,
                        name: column.name.clone(),
                        column_profile_id,
                    }
                })
                .collect();
            TableSchema {
                id: schema.id,
                name: schema.name.clone(),
                columns,
            }
        })
        .collect();
    (schemas, catalog)
}

// --- Schema lowering ---

#[test]
fn test_lower_table_schema() {
    let prog = compile("table balances { balance: u64 }");
    assert_eq!(prog.schemas.len(), 1);
    assert_eq!(prog.schemas[0].id, TableId(0));
    assert_eq!(prog.schemas[0].name, "balances");
    assert_eq!(prog.schemas[0].columns[0].id, ColId(0));
    assert_eq!(prog.schemas[0].columns[0].type_id, TYPE_U64_ID);
}

#[test]
fn test_lower_multiple_tables() {
    let prog = compile("table a { x: u64 }\ntable b { y: bool }");
    assert_eq!(prog.schemas.len(), 2);
    assert_eq!(prog.schemas[0].id, TableId(0));
    assert_eq!(prog.schemas[1].id, TableId(1));
    assert!(prog.column_schemes.is_empty());
}

#[test]
fn test_lower_non_default_column_schemes() {
    let prog = compile("table t { a: u64 @ssmc, b: u64 @smt, c: u64 @scheme(42) }");
    assert_eq!(prog.column_schemes.len(), 2);
    assert_eq!(prog.column_schemes[0].table_id, TableId(0));
    assert_eq!(prog.column_schemes[0].col_id, ColId(1));
    assert_eq!(prog.column_schemes[0].scheme_id, SchemeId::SMT);
    assert_eq!(prog.column_schemes[1].col_id, ColId(2));
    assert_eq!(prog.column_schemes[1].scheme_id, SchemeId(42));
}

#[test]
fn test_lower_duplicate_table_error() {
    let tokens = lex("table a { x: u64 }\ntable a { y: bool }").unwrap();
    let ast = parse(tokens).unwrap();
    let err = lower(&ast).unwrap_err();
    assert!(err.iter().any(|e| e.kind == ErrorKind::DuplicateTable));
}

// --- Simple tx ---

#[test]
fn test_lower_empty_tx() {
    let prog = compile("tx noop() {}");
    assert_eq!(prog.tx_types.len(), 1);
    assert_eq!(prog.tx_types[0].id, TxTypeId(0));
    assert_eq!(prog.tx_types[0].name, "noop");
    assert!(prog.tx_types[0].body.is_empty());
}

// --- Read + Write ---

#[test]
fn test_lower_read_write() {
    let source = "\
table t { v: u64 }
tx rw(id: u64, val: u64) {
    let x = t[id].v
    t[id].v = val
}";
    let prog = compile(source);
    let body = &prog.tx_types[0].body;
    assert_eq!(body.len(), 2);
    assert!(matches!(
        &body[0],
        Instruction::Read {
            dst_val: 0,
            dst_is_null: 1,
            table,
            row: RowExpr::Param(0),
            col,
        } if *table == TableId(0) && *col == ColId(0)
    ));
    assert!(matches!(
        &body[1],
        Instruction::Write {
            table,
            row: RowExpr::Param(0),
            col,
            src_val: ValueExpr::Param(1),
            src_is_null: ValueExpr::Literal(v),
        } if *table == TableId(0) && *col == ColId(0)
            && *v == portable_bool(false)
    ));
}

// --- Arithmetic ---

#[test]
fn test_lower_arithmetic() {
    let source = "\
table t { v: u64 }
tx add_one(id: u64) {
    let x = t[id].v
    let y = x + 1
    t[id].v = y
}";
    let prog = compile(source);
    let body = &prog.tx_types[0].body;
    assert_eq!(body.len(), 3);
    // Read uses slots 0 (val) and 1 (is_null)
    assert!(matches!(
        &body[0],
        Instruction::Read {
            dst_val: 0,
            dst_is_null: 1,
            ..
        }
    ));
    assert!(matches!(
        &body[1],
        Instruction::Arith {
            dst: 2,
            op: ArithOp::Add,
            lhs: ValueExpr::Slot(0),
            rhs: ValueExpr::Literal(v),
        }
            if *v == portable_u64(1)
    ));
    assert!(matches!(
        &body[2],
        Instruction::Write {
            src_val: ValueExpr::Slot(2),
            src_is_null: ValueExpr::Literal(v),
            ..
        }
            if *v == portable_bool(false)
    ));
}

#[test]
fn custom_numeric_descriptor_supports_unary_negation_lowering() {
    let prog = compile_with_registry_for_test(
        "tx negate(x: u64) { let y = -x }",
        &custom_numeric_registry(),
    );
    let body = &prog.tx_types[0].body;
    assert_eq!(body.len(), 1);
    assert!(matches!(
        &body[0],
        Instruction::Arith {
            dst: 0,
            op: ArithOp::Sub,
            lhs: ValueExpr::Literal(v),
            rhs: ValueExpr::Param(0),
        } if *v == portable_zero(CUSTOM_SOURCE_NUMERIC_TYPE_ID)
    ));
    assert_eq!(
        prog.tx_types[0].param_schema[0].type_id,
        CUSTOM_SOURCE_NUMERIC_TYPE_ID
    );
}

#[test]
fn custom_nullable_numeric_descriptor_supports_null_assignment_lowering() {
    let prog = compile_with_registry_for_test(
        "table t { v: u64 }\ntx clear() { t[0].v = null }",
        &custom_numeric_registry(),
    );
    let body = &prog.tx_types[0].body;
    assert_eq!(body.len(), 1);
    assert!(matches!(
        &body[0],
        Instruction::Write {
            src_val: ValueExpr::Literal(v),
            src_is_null: ValueExpr::Literal(flag),
            ..
        } if *v == portable_zero(CUSTOM_SOURCE_NUMERIC_TYPE_ID)
            && *flag == portable_bool(true)
    ));
    assert_eq!(
        prog.schemas[0].columns[0].type_id,
        CUSTOM_SOURCE_NUMERIC_TYPE_ID
    );
}

#[test]
fn opaque_zero_rule_fails_closed_for_null_assignment() {
    let errors = compile_with_registry_err(
        "table t { v: u64 }\ntx clear() { t[0].v = null }",
        &opaque_zero_registry(),
    );
    assert!(errors.iter().any(|err| {
        err.kind == ErrorKind::TypeMismatch
            && err.message.contains("descriptor-synthesizable zero")
            && err.message.contains("opaque zero-value rule")
    }));
}

#[test]
fn unsupported_zero_synthesis_fails_closed_for_unary_negation() {
    let errors =
        compile_with_registry_err("tx negate(x: u64) { let y = -x }", &small_int_registry());
    assert!(errors.iter().any(|err| {
        err.kind == ErrorKind::TypeMismatch
            && err.message.contains("statically synthesizable zero")
            && err.message.contains("incompatible zero-value rule")
    }));
}

// --- Assert ---

#[test]
fn test_lower_assert_gte() {
    let source = "\
tx check(x: u64, y: u64) {
    assert x >= y
}";
    let prog = compile(source);
    let body = &prog.tx_types[0].body;
    assert_eq!(body.len(), 2);
    assert!(matches!(
        &body[0],
        Instruction::Cmp {
            dst: 0,
            op: CmpOp::Gte,
            lhs: ValueExpr::Param(0),
            rhs: ValueExpr::Param(1),
        }
    ));
    assert!(matches!(
        &body[1],
        Instruction::Assert {
            cond: ValueExpr::Slot(0),
        }
    ));
}

#[test]
fn test_lower_assert_not_null() {
    let source = "\
table t { v: u64 }
tx check(id: u64) {
    let x = t[id].v
    assert x != null
}";
    let prog = compile(source);
    let body = &prog.tx_types[0].body;
    // x != null -> Cmp(Eq, is_null_slot, Bool(false)) + Assert
    // Read uses slots 0 (val) and 1 (is_null), Cmp uses slot 2
    assert_eq!(body.len(), 3);
    assert!(matches!(
        &body[1],
        Instruction::Cmp {
            dst: 2,
            op: CmpOp::Eq,
            lhs: ValueExpr::Slot(1),
            rhs: ValueExpr::Literal(v),
        }
            if *v == portable_bool(false)
    ));
    assert!(matches!(
        &body[2],
        Instruction::Assert {
            cond: ValueExpr::Slot(2),
        }
    ));
}

#[test]
fn test_lower_assert_eq_null() {
    let source = "\
table t { v: u64 }
tx check(id: u64) {
    let x = t[id].v
    assert x == null
}";
    let prog = compile(source);
    let body = &prog.tx_types[0].body;
    // x == null -> Cmp(Eq, is_null_slot, Bool(true)) + Assert
    // Read uses slots 0 (val) and 1 (is_null), Cmp uses slot 2
    assert_eq!(body.len(), 3);
    assert!(matches!(
        &body[1],
        Instruction::Cmp {
            dst: 2,
            op: CmpOp::Eq,
            lhs: ValueExpr::Slot(1),
            rhs: ValueExpr::Literal(v),
        }
            if *v == portable_bool(true)
    ));
    assert!(matches!(
        &body[2],
        Instruction::Assert {
            cond: ValueExpr::Slot(2),
        }
    ));
}

// --- Hash ---

#[test]
fn test_lower_hash() {
    let source = "tx h(a: u64, b: u64) { let digest = hash(a, b) }";
    let prog = compile(source);
    let body = &prog.tx_types[0].body;
    assert!(matches!(
        &body[0],
        Instruction::Hash { dst: 0, inputs } if inputs.len() == 2
    ));
}

// --- Emit ---

#[test]
fn test_lower_emit() {
    let source = "tx e(a: u64) { emit \"test\" (a) }";
    let prog = compile(source);
    let body = &prog.tx_types[0].body;
    assert!(matches!(
        &body[0],
        Instruction::Emit { topic, data } if topic == b"test" && data.len() == 1
    ));
}

// --- DivMod ---

#[test]
fn test_lower_divmod() {
    let source = "tx d(a: u64, b: u64) { let (q, r) = divmod(a, b) }";
    let prog = compile(source);
    let body = &prog.tx_types[0].body;
    assert!(matches!(
        &body[0],
        Instruction::DivMod {
            dst_q: 0,
            dst_r: 1,
            lhs: ValueExpr::Param(0),
            rhs: ValueExpr::Param(1),
        }
    ));
}

// --- Div and Mod operators ---

#[test]
fn test_lower_div_operator() {
    let source = "tx d(a: u64, b: u64) { let q = a / b }";
    let prog = compile(source);
    let body = &prog.tx_types[0].body;
    // Should emit DivMod with dst_q as the used slot.
    assert!(matches!(
        &body[0],
        Instruction::DivMod {
            dst_q: 0,
            dst_r: 1,
            ..
        }
    ));
}

// --- Undefined variable ---

#[test]
fn test_lower_undefined_variable() {
    let tokens = lex("tx t() { assert x >= 0 }").unwrap();
    let ast = parse(tokens).unwrap();
    let err = lower(&ast).unwrap_err();
    assert!(err.iter().any(|e| e.kind == ErrorKind::UndefinedVariable));
}

// --- Undefined table ---

#[test]
fn test_lower_undefined_table() {
    let tokens = lex("tx t(id: u64) { let x = foo[id].bar }").unwrap();
    let ast = parse(tokens).unwrap();
    let err = lower(&ast).unwrap_err();
    assert!(err.iter().any(|e| e.kind == ErrorKind::UndefinedTable));
}

#[test]
fn test_lower_row_key_param_must_be_u64() {
    let tokens = lex("table t { v: u64 }\ntx bad(id: bool) { t[id].v = 1 }").unwrap();
    let ast = parse(tokens).unwrap();
    let err = lower(&ast).unwrap_err();
    assert!(err.iter().any(|e| e.kind == ErrorKind::TypeMismatch));
}

#[test]
fn test_lower_row_key_alias_bool_rejected() {
    let tokens = lex("table t { v: u64 }\ntx bad(id: u64) { let b = true\nt[b].v = 1 }").unwrap();
    let ast = parse(tokens).unwrap();
    let err = lower(&ast).unwrap_err();
    assert!(err.iter().any(|e| e.kind == ErrorKind::TypeMismatch));
}

// --- Full transfer ---

#[test]
fn test_lower_transfer() {
    let source = "\
table balances { balance: u64 }

tx transfer(from: u64, to: u64, amount: u64) {
    let sender_bal = balances[from].balance
    let recv_bal = balances[to].balance
    assert sender_bal >= amount
    let new_sender = sender_bal - amount
    let new_recv = recv_bal + amount
    balances[from].balance = new_sender
    balances[to].balance = new_recv
}";
    let prog = compile(source);
    let tx = &prog.tx_types[0];
    assert_eq!(tx.name, "transfer");
    assert_eq!(tx.param_schema.len(), 3);
    assert_eq!(tx.body.len(), 8);

    // Verify exact IR output matches hand-written transfer.
    // Read sender_bal: slots 0 (val), 1 (is_null)
    // Read recv_bal:   slots 2 (val), 3 (is_null)
    // Cmp gte:         slot 4
    // Arith Sub:       slot 5
    // Arith Add:       slot 6
    assert_eq!(
        tx.body,
        vec![
            Instruction::Read {
                dst_val: 0,
                dst_is_null: 1,
                table: TableId(0),
                row: RowExpr::Param(0),
                col: ColId(0),
            },
            Instruction::Read {
                dst_val: 2,
                dst_is_null: 3,
                table: TableId(0),
                row: RowExpr::Param(1),
                col: ColId(0),
            },
            Instruction::Cmp {
                dst: 4,
                op: CmpOp::Gte,
                lhs: ValueExpr::Slot(0),
                rhs: ValueExpr::Param(2),
            },
            Instruction::Assert {
                cond: ValueExpr::Slot(4),
            },
            Instruction::Arith {
                dst: 5,
                op: ArithOp::Sub,
                lhs: ValueExpr::Slot(0),
                rhs: ValueExpr::Param(2),
            },
            Instruction::Arith {
                dst: 6,
                op: ArithOp::Add,
                lhs: ValueExpr::Slot(2),
                rhs: ValueExpr::Param(2),
            },
            Instruction::Write {
                table: TableId(0),
                row: RowExpr::Param(0),
                col: ColId(0),
                src_val: ValueExpr::Slot(5),
                src_is_null: lit_bool(false),
            },
            Instruction::Write {
                table: TableId(0),
                row: RowExpr::Param(1),
                col: ColId(0),
                src_val: ValueExpr::Slot(6),
                src_is_null: lit_bool(false),
            },
        ]
    );
}

// --- Alias (let x = param) ---

#[test]
fn test_lower_alias_no_instruction() {
    let source = "\
tx t(x: u64, y: u64) {
    let a = x
    let b = y
    assert a >= b
}";
    let prog = compile(source);
    let body = &prog.tx_types[0].body;
    // `let a = x` and `let b = y` should NOT emit instructions.
    // Cmp + Assert = 2 instructions.
    assert_eq!(body.len(), 2);
    assert!(matches!(
        &body[0],
        Instruction::Cmp {
            dst: 0,
            op: CmpOp::Gte,
            lhs: ValueExpr::Param(0),
            rhs: ValueExpr::Param(1),
        }
    ));
    assert!(matches!(
        &body[1],
        Instruction::Assert {
            cond: ValueExpr::Slot(0),
        }
    ));
}

#[test]
fn test_lower_alias_bool() {
    let source = "\
tx t(flag: bool) {
    let x = flag
    assert x
}";
    let prog = compile(source);
    let body = &prog.tx_types[0].body;
    // assert x -> Assert { cond: Param(0) } (alias resolves directly)
    assert_eq!(body.len(), 1);
    assert!(matches!(
        &body[0],
        Instruction::Assert {
            cond: ValueExpr::Param(0),
        }
    ));
}

// --- Compound expression in write ---

#[test]
fn test_lower_inline_arithmetic_in_write() {
    let source = "\
table t { v: u64 }
tx inc(id: u64, amount: u64) {
    let x = t[id].v
    t[id].v = x + amount
}";
    let prog = compile(source);
    let body = &prog.tx_types[0].body;
    assert_eq!(body.len(), 3);
    // Read: slots 0 (val), 1 (is_null)
    assert!(matches!(
        &body[0],
        Instruction::Read {
            dst_val: 0,
            dst_is_null: 1,
            ..
        }
    ));
    assert!(matches!(
        &body[1],
        Instruction::Arith {
            dst: 2,
            op: ArithOp::Add,
            ..
        }
    ));
    assert!(matches!(
        &body[2],
        Instruction::Write {
            src_val: ValueExpr::Slot(2),
            src_is_null: ValueExpr::Literal(v),
            ..
        }
            if *v == portable_bool(false)
    ));
}

// --- Select ---

#[test]
fn test_lower_select() {
    let source = "\
table t { a: u64, b: u64 }
tx s(id: u64, flag: bool) {
    let x = t[id].a
    let y = t[id].b
    let result = select(flag, x, y)
    t[id].a = result
}";
    let prog = compile(source);
    let body = &prog.tx_types[0].body;
    assert_eq!(body.len(), 4);
    // Read x: slots 0 (val), 1 (is_null)
    // Read y: slots 2 (val), 3 (is_null)
    // Select: slot 4
    assert!(matches!(
        &body[2],
        Instruction::Select {
            dst: 4,
            cond: ValueExpr::Param(1),
            if_true: ValueExpr::Slot(0),
            if_false: ValueExpr::Slot(2),
        }
    ));
}

#[test]
fn test_lower_select_literal_branches() {
    let source = "tx s(flag: bool) { let x = select(flag, 42, 0) }";
    let prog = compile(source);
    let body = &prog.tx_types[0].body;
    assert_eq!(body.len(), 1);
    assert!(matches!(
        &body[0],
        Instruction::Select {
            dst: 0,
            cond: ValueExpr::Param(0),
            if_true: ValueExpr::Literal(t),
            if_false: ValueExpr::Literal(f),
        }
            if *t == portable_u64(42) && *f == portable_u64(0)
    ));
}

// --- SSA validation of lowered IR ---

#[test]
fn test_lowered_ir_passes_ssa_validation() {
    // Verify that the lowered transfer program passes Program::register() validation.
    // Uses literal row keys (0, 1) so that NF-4 aliasing is provably distinct.
    let source = "\
table balances { balance: u64 }

tx transfer(amount: u64) {
    let sender_bal = balances[0].balance
    let recv_bal = balances[1].balance
    assert sender_bal >= amount
    let new_sender = sender_bal - amount
    let new_recv = recv_bal + amount
    balances[0].balance = new_sender
    balances[1].balance = new_recv
}";
    let compiled = compile(source);
    let (sealed_schemas, profile_catalog) = seal_schemas(&compiled);
    let mut prog = Program::with_profile_catalog(profile_catalog);
    for schema in &sealed_schemas {
        prog.add_schema(schema.clone());
    }
    for tx_type in &compiled.tx_types {
        prog.register(tx_type.clone())
            .unwrap_or_else(|e| panic!("lowered IR failed SSA validation: {e}"));
    }
    // Verify type info was inferred (slot 0 = val U64, slot 1 = is_null Bool)
    let info = prog.type_info(TxTypeId(0)).unwrap();
    assert_eq!(info.slot_types[0], Some(TYPE_U64_ID));
    assert_eq!(info.slot_types[1], Some(TYPE_BOOL_ID));
}

#[test]
fn test_lowered_select_passes_ssa_validation() {
    let source = "\
table t { a: u64, b: u64 }
tx s(id: u64, flag: bool) {
    let x = t[id].a
    let y = t[id].b
    let result = select(flag, x, y)
    t[id].a = result
}";
    let compiled = compile(source);
    let (sealed_schemas, profile_catalog) = seal_schemas(&compiled);
    let mut prog = Program::with_profile_catalog(profile_catalog);
    for schema in &sealed_schemas {
        prog.add_schema(schema.clone());
    }
    for tx_type in &compiled.tx_types {
        prog.register(tx_type.clone())
            .unwrap_or_else(|e| panic!("lowered Select IR failed SSA validation: {e}"));
    }
}

// --- Precompile ---

#[test]
fn test_lower_precompile_basic() {
    let source = "tx t(x: u64) { @precompile(1, [out], x) }";
    let prog = compile_with_precompiles(
        source,
        &[(
            PrecompileId(1),
            PrecompileSignature::new(
                vec![profile(TYPE_U64_ID, ENCODING_U64_ID)],
                vec![profile(TYPE_U64_ID, ENCODING_U64_ID)],
            ),
        )],
    );
    let body = &prog.tx_types[0].body;
    assert_eq!(body.len(), 1);
    assert!(matches!(
        &body[0],
        Instruction::Precompile {
            id,
            dst_slots,
            inputs,
        } if id.0 == 1 && dst_slots == &[0] && inputs.len() == 1
    ));
}

#[test]
fn test_lower_precompile_unknown_id_error() {
    let tokens = lex("tx t(x: u64) { @precompile(1, [out], x) }").unwrap();
    let ast = parse(tokens).unwrap();
    let registry = builtin_semantic_registry().expect("built-in semantic registry");
    let err =
        lower_with_registry_and_precompiles(&ast, &registry, &std::collections::BTreeMap::new())
            .unwrap_err();
    assert!(err.iter().any(|e| e.kind == ErrorKind::UndefinedPrecompile));
}

#[test]
fn test_lower_precompile_input_arity_error() {
    let tokens = lex("tx t(x: u64) { @precompile(1, [out], x) }").unwrap();
    let ast = parse(tokens).unwrap();
    let registry = builtin_semantic_registry().expect("built-in semantic registry");
    let precompiles = [(
        PrecompileId(1),
        PrecompileSignature::new(
            vec![
                profile(TYPE_U64_ID, ENCODING_U64_ID),
                profile(TYPE_U64_ID, ENCODING_U64_ID),
            ],
            vec![profile(TYPE_U64_ID, ENCODING_U64_ID)],
        ),
    )]
    .into_iter()
    .collect();
    let err = lower_with_registry_and_precompiles(&ast, &registry, &precompiles).unwrap_err();
    assert!(err.iter().any(|e| {
        e.kind == ErrorKind::TypeMismatch
            && e.message.contains("expects 2 inputs but 1 were provided")
    }));
}

#[test]
fn test_lower_precompile_input_type_error() {
    let tokens = lex("tx t(flag: bool) { @precompile(1, [out], flag) }").unwrap();
    let ast = parse(tokens).unwrap();
    let registry = builtin_semantic_registry().expect("built-in semantic registry");
    let precompiles = [(
        PrecompileId(1),
        PrecompileSignature::new(
            vec![profile(TYPE_U64_ID, ENCODING_U64_ID)],
            vec![profile(TYPE_U64_ID, ENCODING_U64_ID)],
        ),
    )]
    .into_iter()
    .collect();
    let err = lower_with_registry_and_precompiles(&ast, &registry, &precompiles).unwrap_err();
    assert!(err.iter().any(|e| {
        e.kind == ErrorKind::TypeMismatch && e.message.contains("input expects type id")
    }));
}

#[test]
fn test_lower_precompile_multi_output() {
    let source = "tx t(a: u64, b: u64) { @precompile(42, [x, y], a, b) }";
    let prog = compile_with_precompiles(
        source,
        &[(
            PrecompileId(42),
            PrecompileSignature::new(
                vec![
                    profile(TYPE_U64_ID, ENCODING_U64_ID),
                    profile(TYPE_U64_ID, ENCODING_U64_ID),
                ],
                vec![
                    profile(TYPE_U64_ID, ENCODING_U64_ID),
                    profile(TYPE_U64_ID, ENCODING_U64_ID),
                ],
            ),
        )],
    );
    let body = &prog.tx_types[0].body;
    assert_eq!(body.len(), 1);
    if let Instruction::Precompile {
        id,
        dst_slots,
        inputs,
    } = &body[0]
    {
        assert_eq!(id.0, 42);
        assert_eq!(dst_slots, &[0, 1]);
        assert_eq!(inputs.len(), 2);
        assert!(matches!(&inputs[0], ValueExpr::Param(0)));
        assert!(matches!(&inputs[1], ValueExpr::Param(1)));
    } else {
        panic!("expected Precompile instruction");
    }
}

#[test]
fn test_lower_precompile_output_slot_typing_follows_signature() {
    let source = "\
tx t(x: u64) {
    @precompile(7, [ok], x)
    assert ok
}";
    let prog = compile_with_precompiles(
        source,
        &[(
            PrecompileId(7),
            PrecompileSignature::new(
                vec![profile(TYPE_U64_ID, ENCODING_U64_ID)],
                vec![profile(TYPE_BOOL_ID, ENCODING_BOOL_ID)],
            ),
        )],
    );
    let body = &prog.tx_types[0].body;
    assert_eq!(body.len(), 2);
    assert!(matches!(&body[0], Instruction::Precompile { .. }));
    assert!(matches!(
        &body[1],
        Instruction::Assert {
            cond: ValueExpr::Slot(0)
        }
    ));
}

#[test]
fn test_lower_precompile_output_usable() {
    // Verify precompile outputs can be referenced in subsequent statements.
    let source = "\
tx t(x: u64) {
    @precompile(1, [result], x)
    assert result == 0
}";
    let prog = compile_with_precompiles(
        source,
        &[(
            PrecompileId(1),
            PrecompileSignature::new(
                vec![profile(TYPE_U64_ID, ENCODING_U64_ID)],
                vec![profile(TYPE_U64_ID, ENCODING_U64_ID)],
            ),
        )],
    );
    let body = &prog.tx_types[0].body;
    // Precompile (slot 0) + Cmp (slot 1) + Assert
    assert_eq!(body.len(), 3);
    assert!(matches!(&body[0], Instruction::Precompile { .. }));
    assert!(matches!(
        &body[1],
        Instruction::Cmp {
            lhs: ValueExpr::Slot(0),
            ..
        }
    ));
}

#[test]
fn test_lower_precompile_duplicate_binding_error() {
    let tokens = lex("tx t(x: u64) { @precompile(1, [x], x) }").unwrap();
    let ast = parse(tokens).unwrap();
    let precompiles = [(
        PrecompileId(1),
        PrecompileSignature::new(
            vec![profile(TYPE_U64_ID, ENCODING_U64_ID)],
            vec![profile(TYPE_U64_ID, ENCODING_U64_ID)],
        ),
    )]
    .into_iter()
    .collect();
    let registry = builtin_semantic_registry().expect("built-in semantic registry");
    let err = lower_with_registry_and_precompiles(&ast, &registry, &precompiles).unwrap_err();
    assert!(err.iter().any(|e| e.kind == ErrorKind::DuplicateBinding));
}

// --- Logical AND in assert ---

#[test]
fn test_lower_assert_and() {
    let source = "\
tx t(x: u64, y: u64) {
    assert x > 0 && y > 0
}";
    let prog = compile(source);
    let body = &prog.tx_types[0].body;
    // Cmp(x>0) -> slot 0, Cmp(y>0) -> slot 1, And -> slot 2, Assert
    assert_eq!(body.len(), 4);
    assert!(matches!(
        &body[0],
        Instruction::Cmp {
            dst: 0,
            op: CmpOp::Gt,
            ..
        }
    ));
    assert!(matches!(
        &body[1],
        Instruction::Cmp {
            dst: 1,
            op: CmpOp::Gt,
            ..
        }
    ));
    assert!(matches!(
        &body[2],
        Instruction::And {
            dst: 2,
            lhs: ValueExpr::Slot(0),
            rhs: ValueExpr::Slot(1),
        }
    ));
    assert!(matches!(
        &body[3],
        Instruction::Assert {
            cond: ValueExpr::Slot(2),
        }
    ));
}
