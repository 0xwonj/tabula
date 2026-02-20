use tabula_core::error::TabulaError;
use tabula_core::{ColId, ColumnDef, RowKey, TableSchema, Value, ValueType};
use tabula_core::{TableId, TxTypeId};
use tabula_ir::{ArithOp, CmpOp, Instruction, ParamDef, Program, RowExpr, TxTypeDef, ValueExpr};

/// NF-compliant transfer: reads row 0 and row 1 of (table 1, col 0),
/// transfers `amount` (param 0) from row 0 to row 1.
fn transfer_def() -> TxTypeDef {
    TxTypeDef {
        id: TxTypeId(1),
        name: "transfer".into(),
        param_schema: vec![ParamDef {
            name: "amount".into(),
            value_type: ValueType::U64,
        }],
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
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
            Instruction::Write {
                table: TableId(1),
                row: RowExpr::Literal(RowKey(1)),
                col: ColId(0),
                src_val: ValueExpr::Slot(6),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
        ],
    }
}

fn balances_schema() -> TableSchema {
    TableSchema {
        id: TableId(1),
        name: "balances".into(),
        columns: vec![ColumnDef {
            id: ColId(0),
            name: "balance".into(),
            value_type: ValueType::U64,
        }],
    }
}

#[test]
fn test_register_valid_program() {
    let mut prog = Program::new();
    prog.register(transfer_def()).unwrap();
    assert!(prog.resolve(TxTypeId(1)).is_ok());
}

#[test]
fn test_type_info_inferred() {
    let mut prog = Program::new();
    prog.register(transfer_def()).unwrap();
    let info = prog.type_info(TxTypeId(1)).unwrap();

    // Slots 0, 2 = Read dst_val → None (unknown without table schema)
    // Slots 1, 3 = Read dst_is_null → Bool
    assert_eq!(info.slot_types[0], None);
    assert_eq!(info.slot_types[1], Some(ValueType::Bool));
    assert_eq!(info.slot_types[2], None);
    assert_eq!(info.slot_types[3], Some(ValueType::Bool));
    // Slot 4 = Cmp → Bool
    assert_eq!(info.slot_types[4], Some(ValueType::Bool));
    // Slot 5 = Sub(Slot(0), Param(0)) → Param(0) is U64 → U64
    assert_eq!(info.slot_types[5], Some(ValueType::U64));
    // Slot 6 = Add(Slot(2), Param(0)) → Param(0) is U64 → U64
    assert_eq!(info.slot_types[6], Some(ValueType::U64));
    assert_eq!(info.max_slot, Some(6));
    assert_eq!(info.param_types, vec![ValueType::U64]);
}

#[test]
fn test_hash_produces_bytes32_type() {
    let def = TxTypeDef {
        id: TxTypeId(2),
        name: "hash_test".into(),
        param_schema: vec![ParamDef {
            name: "input".into(),
            value_type: ValueType::U64,
        }],
        body: vec![Instruction::Hash {
            dst: 0,
            inputs: vec![ValueExpr::Param(0)],
        }],
    };
    let mut prog = Program::new();
    prog.register(def).unwrap();
    let info = prog.type_info(TxTypeId(2)).unwrap();
    assert_eq!(info.slot_types[0], Some(ValueType::Bytes32));
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
            src_val: ValueExpr::Literal(Value::U64(1)),
            src_is_null: ValueExpr::Literal(Value::Bool(false)),
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
        param_schema: vec![ParamDef {
            name: "row".into(),
            value_type: ValueType::Bool,
        }],
        body: vec![Instruction::Write {
            table: TableId(1),
            row: RowExpr::Param(0),
            col: ColId(0),
            src_val: ValueExpr::Literal(Value::U64(1)),
            src_is_null: ValueExpr::Literal(Value::Bool(false)),
        }],
    };
    let mut prog = Program::new();
    prog.add_schema(balances_schema());
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
                lhs: ValueExpr::Literal(Value::Bool(true)),
                rhs: ValueExpr::Literal(Value::Bool(false)),
            },
            Instruction::Write {
                table: TableId(1),
                row: RowExpr::Slot(0),
                col: ColId(0),
                src_val: ValueExpr::Literal(Value::U64(1)),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
        ],
    };
    let mut prog = Program::new();
    prog.add_schema(balances_schema());
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
            rhs: ValueExpr::Literal(Value::U64(1)),
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
            lhs: ValueExpr::Literal(Value::I64(10)),
            rhs: ValueExpr::Literal(Value::I64(20)),
        }],
    };
    let mut prog = Program::new();
    prog.register(def).unwrap();
    let info = prog.type_info(TxTypeId(6)).unwrap();
    assert_eq!(info.slot_types[0], Some(ValueType::I64));
}

#[test]
fn test_operand_type_mismatch_rejected() {
    let def = TxTypeDef {
        id: TxTypeId(7),
        name: "bad_add".into(),
        param_schema: vec![
            ParamDef {
                name: "a".into(),
                value_type: ValueType::I64,
            },
            ParamDef {
                name: "b".into(),
                value_type: ValueType::U64,
            },
        ],
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
    let mut prog = Program::new();
    prog.add_schema(balances_schema());
    prog.register(transfer_def()).unwrap();
    let info = prog.type_info(TxTypeId(1)).unwrap();
    assert_eq!(info.slot_types[0], Some(ValueType::U64));
    assert_eq!(info.slot_types[1], Some(ValueType::Bool));
    assert_eq!(info.slot_types[2], Some(ValueType::U64));
    assert_eq!(info.slot_types[3], Some(ValueType::Bool));
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
            src_val: ValueExpr::Literal(Value::Bool(true)), // schema expects U64
            src_is_null: ValueExpr::Literal(Value::Bool(false)),
        }],
    };
    let mut prog = Program::new();
    prog.add_schema(balances_schema());
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
    let mut prog = Program::new();
    prog.add_schema(balances_schema());
    prog.register(def).unwrap();
}

#[test]
fn test_no_schema_read_type_unknown() {
    let mut prog = Program::new();
    prog.register(transfer_def()).unwrap();
    let info = prog.type_info(TxTypeId(1)).unwrap();
    assert_eq!(info.slot_types[0], None);
    assert_eq!(info.slot_types[1], Some(ValueType::Bool));
}

#[test]
fn test_lookup_type_from_schema() {
    let schema = TableSchema {
        id: TableId(99),
        name: "config".into(),
        columns: vec![ColumnDef {
            id: ColId(0),
            name: "flag".into(),
            value_type: ValueType::Bool,
        }],
    };
    let def = TxTypeDef {
        id: TxTypeId(12),
        name: "lookup_test".into(),
        param_schema: vec![ParamDef {
            name: "key".into(),
            value_type: ValueType::U64,
        }],
        body: vec![Instruction::Lookup {
            dst: 0,
            static_table: TableId(99),
            col: ColId(0),
            row: RowExpr::Param(0),
        }],
    };
    let mut prog = Program::new();
    prog.add_schema(schema);
    prog.register(def).unwrap();
    let info = prog.type_info(TxTypeId(12)).unwrap();
    assert_eq!(info.slot_types[0], Some(ValueType::Bool));
}

#[test]
fn test_select_type_inference() {
    let def = TxTypeDef {
        id: TxTypeId(16),
        name: "select_test".into(),
        param_schema: vec![
            ParamDef {
                name: "flag".into(),
                value_type: ValueType::Bool,
            },
            ParamDef {
                name: "a".into(),
                value_type: ValueType::U64,
            },
            ParamDef {
                name: "b".into(),
                value_type: ValueType::U64,
            },
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
    assert_eq!(info.slot_types[0], Some(ValueType::U64));
}

#[test]
fn test_select_branch_type_mismatch_rejected() {
    let def = TxTypeDef {
        id: TxTypeId(17),
        name: "select_mismatch".into(),
        param_schema: vec![
            ParamDef {
                name: "flag".into(),
                value_type: ValueType::Bool,
            },
            ParamDef {
                name: "a".into(),
                value_type: ValueType::U64,
            },
            ParamDef {
                name: "b".into(),
                value_type: ValueType::I64,
            },
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
        param_schema: vec![ParamDef {
            name: "x".into(),
            value_type: ValueType::U64,
        }],
        body: vec![Instruction::Select {
            dst: 0,
            cond: ValueExpr::Param(0), // U64, not Bool
            if_true: ValueExpr::Literal(Value::U64(1)),
            if_false: ValueExpr::Literal(Value::U64(2)),
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
        param_schema: vec![
            ParamDef {
                name: "a".into(),
                value_type: ValueType::U64,
            },
            ParamDef {
                name: "b".into(),
                value_type: ValueType::U64,
            },
        ],
        body: vec![
            Instruction::Arith {
                dst: 0,
                op: ArithOp::Add,
                lhs: ValueExpr::Param(0),
                rhs: ValueExpr::Literal(Value::U64(1)),
            },
            Instruction::Arith {
                dst: 0, // SSA violation
                op: ArithOp::Add,
                lhs: ValueExpr::Param(1),
                rhs: ValueExpr::Literal(Value::U64(2)),
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
        param_schema: vec![ParamDef {
            name: "x".into(),
            value_type: ValueType::U64,
        }],
        body: vec![
            Instruction::Arith {
                dst: 0,
                op: ArithOp::Add,
                lhs: ValueExpr::Param(0),
                rhs: ValueExpr::Literal(Value::U64(1)),
            },
            Instruction::Arith {
                dst: 1,
                op: ArithOp::Add,
                lhs: ValueExpr::Slot(0),
                rhs: ValueExpr::Literal(Value::U64(2)),
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
        param_schema: vec![ParamDef {
            name: "x".into(),
            value_type: ValueType::U64,
        }],
        body: vec![
            Instruction::Arith {
                dst: 0,
                op: ArithOp::Add,
                lhs: ValueExpr::Param(0),
                rhs: ValueExpr::Literal(Value::U64(1)),
            },
            Instruction::DivMod {
                dst_q: 1,
                dst_r: 0, // SSA violation
                lhs: ValueExpr::Slot(0),
                rhs: ValueExpr::Literal(Value::U64(3)),
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
        param_schema: vec![
            ParamDef {
                name: "a".into(),
                value_type: ValueType::U64,
            },
            ParamDef {
                name: "b".into(),
                value_type: ValueType::U64,
            },
        ],
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
    let mut prog = Program::new();
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
    let mut prog = Program::new();
    prog.register(def).unwrap(); // should succeed after canonicalization
}

#[test]
fn test_nf2_duplicate_write_rejected() {
    let def = TxTypeDef {
        id: TxTypeId(31),
        name: "dup_write".into(),
        param_schema: vec![ParamDef {
            name: "v".into(),
            value_type: ValueType::U64,
        }],
        body: vec![
            Instruction::Write {
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Param(0),
                src_val: ValueExpr::Literal(Value::U64(1)),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
            Instruction::Write {
                table: TableId(1),
                col: ColId(0),
                row: RowExpr::Param(0),
                src_val: ValueExpr::Literal(Value::U64(2)),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
        ],
    };
    let mut prog = Program::new();
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
                src_val: ValueExpr::Literal(Value::U64(42)),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
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
    let mut prog = Program::new();
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
        param_schema: vec![ParamDef {
            name: "row".into(),
            value_type: ValueType::U64,
        }],
        body: vec![
            Instruction::Arith {
                dst: 0,
                op: ArithOp::Add,
                lhs: ValueExpr::Param(0),
                rhs: ValueExpr::Literal(Value::U64(1)),
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
                src_val: ValueExpr::Literal(Value::U64(99)),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
        ],
    };
    let mut prog = Program::new();
    prog.register(def).unwrap(); // succeeds: canonicalize inserts guard
}

#[test]
fn test_nf4_read_read_ambiguous_allowed() {
    // Read-read ambiguous pairs are safe under SSA — no guard needed.
    let def = TxTypeDef {
        id: TxTypeId(34),
        name: "diff_params".into(),
        param_schema: vec![
            ParamDef {
                name: "a".into(),
                value_type: ValueType::U64,
            },
            ParamDef {
                name: "b".into(),
                value_type: ValueType::U64,
            },
        ],
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
    let mut prog = Program::new();
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
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
        ],
    };
    let mut prog = Program::new();
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
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
        ],
    };
    let mut prog = Program::new();
    prog.register(def).unwrap();
}

#[test]
fn test_nf_different_tables_no_conflict() {
    let def = TxTypeDef {
        id: TxTypeId(37),
        name: "diff_tables".into(),
        param_schema: vec![ParamDef {
            name: "r".into(),
            value_type: ValueType::U64,
        }],
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
    let mut prog = Program::new();
    prog.register(def).unwrap();
}

#[test]
fn test_nf_different_columns_no_conflict() {
    let def = TxTypeDef {
        id: TxTypeId(38),
        name: "diff_cols".into(),
        param_schema: vec![ParamDef {
            name: "r".into(),
            value_type: ValueType::U64,
        }],
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
    let mut prog = Program::new();
    prog.register(def).unwrap();
}

#[test]
fn test_nf_same_param_read_canonicalized() {
    // Same param → same cell → canonicalize deduplicates → succeeds.
    let def = TxTypeDef {
        id: TxTypeId(39),
        name: "same_param_read".into(),
        param_schema: vec![ParamDef {
            name: "r".into(),
            value_type: ValueType::U64,
        }],
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
    let mut prog = Program::new();
    prog.register(def).unwrap(); // should succeed after canonicalization
}
