#![allow(missing_docs)]
use tabula_core::error::TabulaError;
use tabula_core::{ColId, ColumnDef, ColumnProfileId, PortableValue, RowKey, TableSchema, TypeId};
use tabula_core::{TableId, TxTypeId};
use tabula_ir::{ArithOp, CmpOp, Instruction, ParamDef, Program, RowExpr, TxTypeDef, ValueExpr};
use tabula_profile::{
    ColumnProfile, CommitmentRole, ENCODING_BOOL_ID, ENCODING_BYTES32_ID, ENCODING_I64_ID,
    ENCODING_U64_ID, ProfileCatalog, SCHEME_PROFILE_SSMC_ID, TYPE_BOOL_ID, TYPE_BYTES32_ID,
    TYPE_I64_ID, TYPE_U64_ID, builtin_catalog,
};

fn some_type(type_id: TypeId) -> Option<TypeId> {
    Some(type_id)
}

fn lit_u64(value: u64) -> ValueExpr {
    ValueExpr::Literal(PortableValue::new(
        TYPE_U64_ID,
        borsh::to_vec(&value).expect("u64 literal"),
    ))
}

fn lit_i64(value: i64) -> ValueExpr {
    ValueExpr::Literal(PortableValue::new(
        TYPE_I64_ID,
        borsh::to_vec(&value).expect("i64 literal"),
    ))
}

fn lit_bool(value: bool) -> ValueExpr {
    ValueExpr::Literal(PortableValue::new(
        TYPE_BOOL_ID,
        borsh::to_vec(&value).expect("bool literal"),
    ))
}

fn param(name: &str, type_id: TypeId) -> ParamDef {
    ParamDef {
        name: name.into(),
        type_id,
    }
}

fn single_column_schema(
    table_id: TableId,
    table_name: &str,
    col_id: ColId,
    col_name: &str,
    type_id: TypeId,
) -> (TableSchema, ProfileCatalog) {
    let mut catalog = builtin_catalog().expect("built-in catalog");
    let type_descriptor = catalog
        .type_descriptor(type_id)
        .cloned()
        .expect("built-in type descriptor");
    let encoding_id = match type_id {
        TYPE_U64_ID => ENCODING_U64_ID,
        TYPE_I64_ID => ENCODING_I64_ID,
        TYPE_BOOL_ID => ENCODING_BOOL_ID,
        TYPE_BYTES32_ID => ENCODING_BYTES32_ID,
        other => panic!("unsupported built-in type id {}", other.0),
    };
    let encoding_profile = catalog
        .encoding_profile(encoding_id)
        .cloned()
        .expect("built-in encoding profile");
    let scheme_profile = catalog
        .scheme_profile(SCHEME_PROFILE_SSMC_ID)
        .cloned()
        .expect("built-in ssmc profile");
    let column_profile = ColumnProfile::new(
        ColumnProfileId(0),
        format!("{table_name}.{col_name}"),
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
        .expect("register column profile");
    (
        TableSchema {
            id: table_id,
            name: table_name.into(),
            columns: vec![ColumnDef {
                id: col_id,
                name: col_name.into(),
                column_profile_id,
            }],
        },
        catalog,
    )
}

fn schemas_with_columns(
    specs: &[(TableId, &str, ColId, &str, TypeId)],
) -> (Vec<TableSchema>, ProfileCatalog) {
    use std::collections::BTreeMap;

    let mut catalog = builtin_catalog().expect("built-in catalog");
    let scheme_profile = catalog
        .scheme_profile(SCHEME_PROFILE_SSMC_ID)
        .cloned()
        .expect("built-in ssmc profile");
    let mut schemas: BTreeMap<TableId, TableSchema> = BTreeMap::new();

    for (index, (table_id, table_name, col_id, col_name, type_id)) in specs.iter().enumerate() {
        let type_descriptor = catalog
            .type_descriptor(*type_id)
            .cloned()
            .expect("built-in type descriptor");
        let encoding_profile = catalog
            .encoding_profile(match *type_id {
                TYPE_U64_ID => ENCODING_U64_ID,
                TYPE_I64_ID => ENCODING_I64_ID,
                TYPE_BOOL_ID => ENCODING_BOOL_ID,
                TYPE_BYTES32_ID => ENCODING_BYTES32_ID,
                other => panic!("unsupported built-in type id {}", other.0),
            })
            .cloned()
            .expect("built-in encoding profile");
        let column_profile = ColumnProfile::new(
            ColumnProfileId(index as u32),
            format!("{table_name}.{col_name}"),
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
            .expect("register column profile");

        schemas
            .entry(*table_id)
            .or_insert_with(|| TableSchema {
                id: *table_id,
                name: (*table_name).into(),
                columns: Vec::new(),
            })
            .columns
            .push(ColumnDef {
                id: *col_id,
                name: (*col_name).into(),
                column_profile_id,
            });
    }

    let mut schemas: Vec<_> = schemas.into_values().collect();
    schemas.sort_by_key(|schema| schema.id);
    for schema in &mut schemas {
        schema.columns.sort_by_key(|column| column.id);
    }
    (schemas, catalog)
}

/// NF-compliant transfer: reads row 0 and row 1 of (table 1, col 0),
/// transfers `amount` (param 0) from row 0 to row 1.
fn transfer_def() -> TxTypeDef {
    TxTypeDef {
        id: TxTypeId(1),
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
                src_is_null: lit_bool(false),
            },
            Instruction::Write {
                table: TableId(1),
                row: RowExpr::Literal(RowKey(1)),
                col: ColId(0),
                src_val: ValueExpr::Slot(6),
                src_is_null: lit_bool(false),
            },
        ],
    }
}

fn balances_program() -> Program {
    let (schema, catalog) =
        single_column_schema(TableId(1), "balances", ColId(0), "balance", TYPE_U64_ID);
    let mut prog = Program::with_profile_catalog(catalog);
    prog.add_schema(schema);
    prog
}

fn program_with_columns(specs: &[(TableId, &str, ColId, &str, TypeId)]) -> Program {
    let (schemas, catalog) = schemas_with_columns(specs);
    let mut prog = Program::with_profile_catalog(catalog);
    for schema in schemas {
        prog.add_schema(schema);
    }
    prog
}

#[test]
fn test_register_valid_program() {
    let mut prog = balances_program();
    prog.register(transfer_def()).unwrap();
    assert!(prog.resolve(TxTypeId(1)).is_ok());
}

#[test]
fn test_type_info_inferred() {
    let mut prog = balances_program();
    prog.register(transfer_def()).unwrap();
    let info = prog.type_info(TxTypeId(1)).unwrap();

    // Slots 0, 2 = Read dst_val → U64 from sealed profile-backed schema.
    // Slots 1, 3 = Read dst_is_null → Bool
    assert_eq!(info.slot_types[0], some_type(TYPE_U64_ID));
    assert_eq!(info.slot_types[1], some_type(TYPE_BOOL_ID));
    assert_eq!(info.slot_types[2], some_type(TYPE_U64_ID));
    assert_eq!(info.slot_types[3], some_type(TYPE_BOOL_ID));
    // Slot 4 = Cmp → Bool
    assert_eq!(info.slot_types[4], some_type(TYPE_BOOL_ID));
    // Slot 5 = Sub(Slot(0), Param(0)) → Param(0) is U64 → U64
    assert_eq!(info.slot_types[5], some_type(TYPE_U64_ID));
    // Slot 6 = Add(Slot(2), Param(0)) → Param(0) is U64 → U64
    assert_eq!(info.slot_types[6], some_type(TYPE_U64_ID));
    assert_eq!(info.max_slot, Some(6));
    assert_eq!(info.param_types, vec![TYPE_U64_ID]);
}

#[test]
fn test_hash_produces_bytes32_type() {
    let def = TxTypeDef {
        id: TxTypeId(2),
        name: "hash_test".into(),
        param_schema: vec![param("input", TYPE_U64_ID)],
        body: vec![Instruction::Hash {
            dst: 0,
            inputs: vec![ValueExpr::Param(0)],
        }],
    };
    let mut prog = Program::new();
    prog.register(def).unwrap();
    let info = prog.type_info(TxTypeId(2)).unwrap();
    assert_eq!(info.slot_types[0], some_type(TYPE_BYTES32_ID));
}

#[test]
fn test_param_out_of_bounds_rejected() {
    let def = TxTypeDef {
        id: TxTypeId(3),
        name: "bad_param".into(),
        param_schema: vec![], // no params
        body: vec![Instruction::Write {
            table: TableId(1),
            row: RowExpr::Param(0), // param 0 doesn't exist
            col: ColId(0),
            src_val: lit_u64(1),
            src_is_null: lit_bool(false),
        }],
    };
    let mut prog = Program::new();
    let err = prog.register(def).unwrap_err();
    assert!(matches!(err, TabulaError::InvalidIr(_)));
}

#[test]
fn test_row_param_non_u64_rejected() {
    let def = TxTypeDef {
        id: TxTypeId(20),
        name: "bad_row_param".into(),
        param_schema: vec![param("row", TYPE_BOOL_ID)],
        body: vec![Instruction::Write {
            table: TableId(1),
            row: RowExpr::Param(0),
            col: ColId(0),
            src_val: lit_u64(1),
            src_is_null: lit_bool(false),
        }],
    };
    let mut prog = balances_program();
    let err = prog.register(def).unwrap_err();
    assert!(matches!(
        err,
        TabulaError::InvalidIr(ref msg) if msg.contains("row expression param")
    ));
}

#[test]
fn test_row_slot_non_u64_rejected() {
    let def = TxTypeDef {
        id: TxTypeId(21),
        name: "bad_row_slot".into(),
        param_schema: vec![],
        body: vec![
            Instruction::Cmp {
                dst: 0,
                op: CmpOp::Eq,
                lhs: lit_bool(true),
                rhs: lit_bool(false),
            },
            Instruction::Write {
                table: TableId(1),
                row: RowExpr::Slot(0),
                col: ColId(0),
                src_val: lit_u64(1),
                src_is_null: lit_bool(false),
            },
        ],
    };
    let mut prog = balances_program();
    let err = prog.register(def).unwrap_err();
    assert!(matches!(
        err,
        TabulaError::InvalidIr(ref msg) if msg.contains("row expression slot")
    ));
}

#[test]
fn test_slot_read_before_assign_rejected() {
    let def = TxTypeDef {
        id: TxTypeId(4),
        name: "bad_slot".into(),
        param_schema: vec![],
        body: vec![Instruction::Arith {
            dst: 1,
            op: ArithOp::Add,
            lhs: ValueExpr::Slot(0), // slot 0 never assigned
            rhs: lit_u64(1),
        }],
    };
    let mut prog = Program::new();
    let err = prog.register(def).unwrap_err();
    assert!(matches!(err, TabulaError::InvalidIr(_)));
}

#[test]
fn test_empty_body_valid() {
    let def = TxTypeDef {
        id: TxTypeId(5),
        name: "noop".into(),
        param_schema: vec![],
        body: vec![],
    };
    let mut prog = Program::new();
    prog.register(def).unwrap();
    let info = prog.type_info(TxTypeId(5)).unwrap();
    assert!(info.slot_types.is_empty());
    assert_eq!(info.max_slot, None);
}

#[test]
fn test_resolve_missing_type() {
    let prog = Program::new();
    let err = prog.resolve(TxTypeId(99)).unwrap_err();
    assert_eq!(err, TabulaError::TxTypeNotFound(TxTypeId(99)));
}

#[test]
fn test_literal_type_inference() {
    let def = TxTypeDef {
        id: TxTypeId(6),
        name: "literal_add".into(),
        param_schema: vec![],
        body: vec![Instruction::Arith {
            dst: 0,
            op: ArithOp::Add,
            lhs: lit_i64(10),
            rhs: lit_i64(20),
        }],
    };
    let mut prog = Program::new();
    prog.register(def).unwrap();
    let info = prog.type_info(TxTypeId(6)).unwrap();
    assert_eq!(info.slot_types[0], some_type(TYPE_I64_ID));
}

#[test]
fn test_operand_type_mismatch_rejected() {
    let def = TxTypeDef {
        id: TxTypeId(7),
        name: "bad_add".into(),
        param_schema: vec![param("a", TYPE_I64_ID), param("b", TYPE_U64_ID)],
        body: vec![Instruction::Arith {
            dst: 0,
            op: ArithOp::Add,
            lhs: ValueExpr::Param(0), // I64
            rhs: ValueExpr::Param(1), // U64
        }],
    };
    let mut prog = Program::new();
    let err = prog.register(def).unwrap_err();
    assert!(matches!(err, TabulaError::InvalidIr(_)));
}

#[test]
fn test_schema_infers_read_type() {
    let mut prog = balances_program();
    prog.register(transfer_def()).unwrap();
    let info = prog.type_info(TxTypeId(1)).unwrap();
    assert_eq!(info.slot_types[0], some_type(TYPE_U64_ID));
    assert_eq!(info.slot_types[1], some_type(TYPE_BOOL_ID));
    assert_eq!(info.slot_types[2], some_type(TYPE_U64_ID));
    assert_eq!(info.slot_types[3], some_type(TYPE_BOOL_ID));
}

#[test]
fn test_schema_write_type_mismatch() {
    let def = TxTypeDef {
        id: TxTypeId(10),
        name: "bad_write".into(),
        param_schema: vec![],
        body: vec![Instruction::Write {
            table: TableId(1),
            row: RowExpr::Literal(RowKey(0)),
            col: ColId(0),
            src_val: lit_bool(true), // schema expects U64
            src_is_null: lit_bool(false),
        }],
    };
    let mut prog = balances_program();
    let err = prog.register(def).unwrap_err();
    assert!(matches!(err, TabulaError::InvalidIr(_)));
}

#[test]
fn test_schema_write_unknown_src_accepted() {
    let def = TxTypeDef {
        id: TxTypeId(11),
        name: "passthrough".into(),
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
                row: RowExpr::Literal(RowKey(0)),
                col: ColId(0),
                src_val: ValueExpr::Slot(0),
                src_is_null: ValueExpr::Slot(1),
            },
        ],
    };
    let mut prog = balances_program();
    prog.register(def).unwrap();
}

#[test]
fn test_no_schema_read_type_unknown() {
    let mut prog = Program::new();
    let err = prog.register(transfer_def()).unwrap_err();
    assert!(matches!(
        err,
        TabulaError::InvalidIr(ref msg) if msg.contains("schema is missing table 1 col 0")
    ));
}

#[test]
fn test_lookup_type_from_schema() {
    let (schema, catalog) =
        single_column_schema(TableId(99), "config", ColId(0), "flag", TYPE_BOOL_ID);
    let def = TxTypeDef {
        id: TxTypeId(12),
        name: "lookup_test".into(),
        param_schema: vec![param("key", TYPE_U64_ID)],
        body: vec![Instruction::Lookup {
            dst: 0,
            static_table: TableId(99),
            col: ColId(0),
            row: RowExpr::Param(0),
        }],
    };
    let mut prog = Program::with_profile_catalog(catalog);
    prog.add_schema(schema);
    prog.register(def).unwrap();
    let info = prog.type_info(TxTypeId(12)).unwrap();
    assert_eq!(info.slot_types[0], some_type(TYPE_BOOL_ID));
}

#[test]
fn test_select_type_inference() {
    let def = TxTypeDef {
        id: TxTypeId(16),
        name: "select_test".into(),
        param_schema: vec![
            param("flag", TYPE_BOOL_ID),
            param("a", TYPE_U64_ID),
            param("b", TYPE_U64_ID),
        ],
        body: vec![Instruction::Select {
            dst: 0,
            cond: ValueExpr::Param(0),
            if_true: ValueExpr::Param(1),
            if_false: ValueExpr::Param(2),
        }],
    };
    let mut prog = Program::new();
    prog.register(def).unwrap();
    let info = prog.type_info(TxTypeId(16)).unwrap();
    assert_eq!(info.slot_types[0], some_type(TYPE_U64_ID));
}

#[test]
fn test_select_branch_type_mismatch_rejected() {
    let def = TxTypeDef {
        id: TxTypeId(17),
        name: "select_mismatch".into(),
        param_schema: vec![
            param("flag", TYPE_BOOL_ID),
            param("a", TYPE_U64_ID),
            param("b", TYPE_I64_ID),
        ],
        body: vec![Instruction::Select {
            dst: 0,
            cond: ValueExpr::Param(0),
            if_true: ValueExpr::Param(1),
            if_false: ValueExpr::Param(2),
        }],
    };
    let mut prog = Program::new();
    let err = prog.register(def).unwrap_err();
    assert!(matches!(err, TabulaError::InvalidIr(ref msg) if msg.contains("type mismatch")));
}

#[test]
fn test_select_non_bool_cond_rejected() {
    let def = TxTypeDef {
        id: TxTypeId(18),
        name: "select_bad_cond".into(),
        param_schema: vec![param("x", TYPE_U64_ID)],
        body: vec![Instruction::Select {
            dst: 0,
            cond: ValueExpr::Param(0), // U64, not Bool
            if_true: lit_u64(1),
            if_false: lit_u64(2),
        }],
    };
    let mut prog = Program::new();
    let err = prog.register(def).unwrap_err();
    assert!(matches!(err, TabulaError::InvalidIr(ref msg) if msg.contains("Bool")));
}

#[test]
fn test_ssa_violation_rejected() {
    let def = TxTypeDef {
        id: TxTypeId(13),
        name: "ssa_violate".into(),
        param_schema: vec![param("a", TYPE_U64_ID), param("b", TYPE_U64_ID)],
        body: vec![
            Instruction::Arith {
                dst: 0,
                op: ArithOp::Add,
                lhs: ValueExpr::Param(0),
                rhs: lit_u64(1),
            },
            Instruction::Arith {
                dst: 0, // SSA violation
                op: ArithOp::Add,
                lhs: ValueExpr::Param(1),
                rhs: lit_u64(2),
            },
        ],
    };
    let mut prog = Program::new();
    let err = prog.register(def).unwrap_err();
    assert!(matches!(err, TabulaError::InvalidIr(ref msg) if msg.contains("SSA violation")));
}

#[test]
fn test_ssa_distinct_slots_accepted() {
    let def = TxTypeDef {
        id: TxTypeId(14),
        name: "ssa_valid".into(),
        param_schema: vec![param("x", TYPE_U64_ID)],
        body: vec![
            Instruction::Arith {
                dst: 0,
                op: ArithOp::Add,
                lhs: ValueExpr::Param(0),
                rhs: lit_u64(1),
            },
            Instruction::Arith {
                dst: 1,
                op: ArithOp::Add,
                lhs: ValueExpr::Slot(0),
                rhs: lit_u64(2),
            },
        ],
    };
    let mut prog = Program::new();
    prog.register(def).unwrap();
}

#[test]
fn test_ssa_divmod_both_slots_unique() {
    let def = TxTypeDef {
        id: TxTypeId(15),
        name: "divmod_ssa".into(),
        param_schema: vec![param("x", TYPE_U64_ID)],
        body: vec![
            Instruction::Arith {
                dst: 0,
                op: ArithOp::Add,
                lhs: ValueExpr::Param(0),
                rhs: lit_u64(1),
            },
            Instruction::DivMod {
                dst_q: 1,
                dst_r: 0, // SSA violation
                lhs: ValueExpr::Slot(0),
                rhs: lit_u64(3),
            },
        ],
    };
    let mut prog = Program::new();
    let err = prog.register(def).unwrap_err();
    assert!(matches!(err, TabulaError::InvalidIr(ref msg) if msg.contains("SSA violation")));
}

#[test]
fn test_ssa_divmod_same_dst_rejected() {
    let def = TxTypeDef {
        id: TxTypeId(19),
        name: "divmod_same_dst".into(),
        param_schema: vec![param("a", TYPE_U64_ID), param("b", TYPE_U64_ID)],
        body: vec![Instruction::DivMod {
            dst_q: 0,
            dst_r: 0, // SSA violation: same slot
            lhs: ValueExpr::Param(0),
            rhs: ValueExpr::Param(1),
        }],
    };
    let mut prog = Program::new();
    let err = prog.register(def).unwrap_err();
    assert!(matches!(err, TabulaError::InvalidIr(ref msg) if msg.contains("SSA violation")));
}

// -----------------------------------------------------------------------
// Normal-form validation tests
// -----------------------------------------------------------------------

#[test]
fn test_nf_transfer_passes() {
    let mut prog = balances_program();
    prog.register(transfer_def()).unwrap();
}

#[test]
fn test_nf1_duplicate_read_canonicalized() {
    // Canonicalize deduplicates reads — this now succeeds instead of failing.
    let def = TxTypeDef {
        id: TxTypeId(30),
        name: "dup_read".into(),
        param_schema: vec![],
        body: vec![
            Instruction::Read {
                dst_val: 0,
                dst_is_null: 1,
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(0)),
            },
            Instruction::Read {
                dst_val: 2,
                dst_is_null: 3,
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(0)),
            },
        ],
    };
    let mut prog = balances_program();
    prog.register(def).unwrap(); // should succeed after canonicalization
}

#[test]
fn test_nf2_duplicate_write_rejected() {
    let def = TxTypeDef {
        id: TxTypeId(31),
        name: "dup_write".into(),
        param_schema: vec![param("v", TYPE_U64_ID)],
        body: vec![
            Instruction::Write {
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Param(0),
                src_val: lit_u64(1),
                src_is_null: lit_bool(false),
            },
            Instruction::Write {
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Param(0),
                src_val: lit_u64(2),
                src_is_null: lit_bool(false),
            },
        ],
    };
    let mut prog = balances_program();
    let err = prog.register(def).unwrap_err();
    assert!(matches!(
        err,
        TabulaError::NfUniqueWrite {
            first: 0,
            second: 1,
            ..
        }
    ));
}

#[test]
fn test_nf3_read_after_write_rejected() {
    let def = TxTypeDef {
        id: TxTypeId(32),
        name: "raw".into(),
        param_schema: vec![],
        body: vec![
            Instruction::Write {
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(5)),
                src_val: lit_u64(42),
                src_is_null: lit_bool(false),
            },
            Instruction::Read {
                dst_val: 0,
                dst_is_null: 1,
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(5)),
            },
        ],
    };
    let mut prog = balances_program();
    let err = prog.register(def).unwrap_err();
    assert!(matches!(
        err,
        TabulaError::NfReadAfterWrite {
            write_at: 0,
            read_at: 1,
            ..
        }
    ));
}

#[test]
fn test_nf4_write_involved_ambiguous_auto_guarded() {
    // Slot(0) vs Param(0) with read+write: canonicalize auto-inserts guard.
    let def = TxTypeDef {
        id: TxTypeId(33),
        name: "ambiguous".into(),
        param_schema: vec![param("row", TYPE_U64_ID)],
        body: vec![
            Instruction::Arith {
                dst: 0,
                op: ArithOp::Add,
                lhs: ValueExpr::Param(0),
                rhs: lit_u64(1),
            },
            Instruction::Read {
                dst_val: 1,
                dst_is_null: 2,
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Slot(0),
            },
            Instruction::Write {
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Param(0),
                src_val: lit_u64(99),
                src_is_null: lit_bool(false),
            },
        ],
    };
    let mut prog = balances_program();
    prog.register(def).unwrap(); // succeeds: canonicalize inserts guard
}

#[test]
fn test_nf4_read_read_ambiguous_allowed() {
    // Read-read ambiguous pairs are safe under SSA — no guard needed.
    let def = TxTypeDef {
        id: TxTypeId(34),
        name: "diff_params".into(),
        param_schema: vec![param("a", TYPE_U64_ID), param("b", TYPE_U64_ID)],
        body: vec![
            Instruction::Read {
                dst_val: 0,
                dst_is_null: 1,
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Param(0),
            },
            Instruction::Read {
                dst_val: 2,
                dst_is_null: 3,
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Param(1),
            },
        ],
    };
    let mut prog = balances_program();
    prog.register(def).unwrap(); // succeeds: read-read ambiguous is safe
}

#[test]
fn test_nf_distinct_literal_rows_accepted() {
    let def = TxTypeDef {
        id: TxTypeId(35),
        name: "distinct_lit".into(),
        param_schema: vec![],
        body: vec![
            Instruction::Read {
                dst_val: 0,
                dst_is_null: 1,
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(0)),
            },
            Instruction::Write {
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(1)),
                src_val: ValueExpr::Slot(0),
                src_is_null: lit_bool(false),
            },
        ],
    };
    let mut prog = balances_program();
    prog.register(def).unwrap();
}

#[test]
fn test_nf_read_then_write_same_cell_accepted() {
    let def = TxTypeDef {
        id: TxTypeId(36),
        name: "read_write".into(),
        param_schema: vec![],
        body: vec![
            Instruction::Read {
                dst_val: 0,
                dst_is_null: 1,
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(0)),
            },
            Instruction::Write {
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(0)),
                src_val: ValueExpr::Slot(0),
                src_is_null: lit_bool(false),
            },
        ],
    };
    let mut prog = balances_program();
    prog.register(def).unwrap();
}

#[test]
fn test_nf_different_tables_no_conflict() {
    let def = TxTypeDef {
        id: TxTypeId(37),
        name: "diff_tables".into(),
        param_schema: vec![param("r", TYPE_U64_ID)],
        body: vec![
            Instruction::Read {
                dst_val: 0,
                dst_is_null: 1,
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Param(0),
            },
            Instruction::Read {
                dst_val: 2,
                dst_is_null: 3,
                table: TableId(2),
                col: ColId(0),
                row: RowExpr::Param(0),
            },
        ],
    };
    let mut prog = program_with_columns(&[
        (TableId(1), "balances", ColId(0), "balance", TYPE_U64_ID),
        (TableId(2), "balances_2", ColId(0), "balance", TYPE_U64_ID),
    ]);
    prog.register(def).unwrap();
}

#[test]
fn test_nf_different_columns_no_conflict() {
    let def = TxTypeDef {
        id: TxTypeId(38),
        name: "diff_cols".into(),
        param_schema: vec![param("r", TYPE_U64_ID)],
        body: vec![
            Instruction::Read {
                dst_val: 0,
                dst_is_null: 1,
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Param(0),
            },
            Instruction::Read {
                dst_val: 2,
                dst_is_null: 3,
                table: TableId(1),
                col: ColId(1),
                row: RowExpr::Param(0),
            },
        ],
    };
    let mut prog = program_with_columns(&[
        (TableId(1), "balances", ColId(0), "balance", TYPE_U64_ID),
        (TableId(1), "balances", ColId(1), "pending", TYPE_U64_ID),
    ]);
    prog.register(def).unwrap();
}

#[test]
fn test_nf_same_param_read_canonicalized() {
    // Same param → same cell → canonicalize deduplicates → succeeds.
    let def = TxTypeDef {
        id: TxTypeId(39),
        name: "same_param_read".into(),
        param_schema: vec![param("r", TYPE_U64_ID)],
        body: vec![
            Instruction::Read {
                dst_val: 0,
                dst_is_null: 1,
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Param(0),
            },
            Instruction::Read {
                dst_val: 2,
                dst_is_null: 3,
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Param(0),
            },
        ],
    };
    let mut prog = balances_program();
    prog.register(def).unwrap(); // should succeed after canonicalization
}
