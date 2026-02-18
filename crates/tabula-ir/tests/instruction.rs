use tabula_core::{ColId, RowKey, TableId, Value};
use tabula_ir::{ArithOp, CmpOp, Instruction, RowExpr, Slot, ValueExpr};

#[test]
fn test_instruction_borsh_round_trip() {
    let instr = Instruction::Read {
        dst_val: 0,
        dst_is_null: 1,
        table: TableId(1),
        col: ColId(0),
        row: RowExpr::Param(0),
    };
    let bytes = borsh::to_vec(&instr).unwrap();
    let decoded: Instruction = borsh::from_slice(&bytes).unwrap();
    assert_eq!(instr, decoded);
}

#[test]
fn test_arith_borsh_round_trip() {
    let instr = Instruction::Arith {
        dst: 0,
        op: ArithOp::Add,
        lhs: ValueExpr::Slot(1),
        rhs: ValueExpr::Literal(Value::U64(10)),
    };
    let bytes = borsh::to_vec(&instr).unwrap();
    let decoded: Instruction = borsh::from_slice(&bytes).unwrap();
    assert_eq!(instr, decoded);
}

#[test]
fn test_cmp_borsh_round_trip() {
    let instr = Instruction::Cmp {
        dst: 2,
        op: CmpOp::Gte,
        lhs: ValueExpr::Slot(0),
        rhs: ValueExpr::Param(0),
    };
    let bytes = borsh::to_vec(&instr).unwrap();
    let decoded: Instruction = borsh::from_slice(&bytes).unwrap();
    assert_eq!(instr, decoded);
}

#[test]
fn test_bool_ops_borsh_round_trip() {
    for instr in [
        Instruction::Not {
            dst: 0,
            src: ValueExpr::Slot(1),
        },
        Instruction::And {
            dst: 0,
            lhs: ValueExpr::Slot(1),
            rhs: ValueExpr::Slot(2),
        },
        Instruction::Or {
            dst: 0,
            lhs: ValueExpr::Literal(Value::Bool(true)),
            rhs: ValueExpr::Slot(1),
        },
    ] {
        let bytes = borsh::to_vec(&instr).unwrap();
        let decoded: Instruction = borsh::from_slice(&bytes).unwrap();
        assert_eq!(instr, decoded);
    }
}

#[test]
fn test_assert_borsh_round_trip() {
    let instr = Instruction::Assert {
        cond: ValueExpr::Slot(5),
    };
    let bytes = borsh::to_vec(&instr).unwrap();
    let decoded: Instruction = borsh::from_slice(&bytes).unwrap();
    assert_eq!(instr, decoded);
}

#[test]
fn test_arith_op_apply() {
    let a = Value::U64(10);
    let b = Value::U64(3);
    assert_eq!(ArithOp::Add.apply(&a, &b).unwrap(), Value::U64(13));
    assert_eq!(ArithOp::Sub.apply(&a, &b).unwrap(), Value::U64(7));
    assert_eq!(ArithOp::Mul.apply(&a, &b).unwrap(), Value::U64(30));
}

#[test]
fn test_cmp_op_apply() {
    let a = Value::U64(5);
    let b = Value::U64(5);
    let c = Value::U64(3);
    assert_eq!(CmpOp::Eq.apply(&a, &b).unwrap(), Value::Bool(true));
    assert_eq!(CmpOp::Ne.apply(&a, &b).unwrap(), Value::Bool(false));
    assert_eq!(CmpOp::Lt.apply(&a, &b).unwrap(), Value::Bool(false));
    assert_eq!(CmpOp::Lte.apply(&a, &b).unwrap(), Value::Bool(true));
    assert_eq!(CmpOp::Gt.apply(&a, &c).unwrap(), Value::Bool(true));
    assert_eq!(CmpOp::Gte.apply(&c, &a).unwrap(), Value::Bool(false));
}

#[test]
fn test_map_slots_rewrites_all_refs() {
    let instr = Instruction::Arith {
        dst: 0,
        op: ArithOp::Add,
        lhs: ValueExpr::Slot(1),
        rhs: ValueExpr::Slot(2),
    };
    let mapped = instr.map_slots(&|s| s + 10);
    assert_eq!(
        mapped,
        Instruction::Arith {
            dst: 10,
            op: ArithOp::Add,
            lhs: ValueExpr::Slot(11),
            rhs: ValueExpr::Slot(12),
        }
    );
}

#[test]
fn test_map_slots_skips_non_slot_exprs() {
    let instr = Instruction::Arith {
        dst: 0,
        op: ArithOp::Add,
        lhs: ValueExpr::Param(0),
        rhs: ValueExpr::Literal(Value::U64(1)),
    };
    let mapped = instr.map_slots(&|s| s + 10);
    assert_eq!(
        mapped,
        Instruction::Arith {
            dst: 10,
            op: ArithOp::Add,
            lhs: ValueExpr::Param(0),
            rhs: ValueExpr::Literal(Value::U64(1)),
        }
    );
}

#[test]
fn test_dst_slots() {
    let read = Instruction::Read {
        dst_val: 0,
        dst_is_null: 1,
        table: TableId(1),
        col: ColId(0),
        row: RowExpr::Param(0),
    };
    assert_eq!(read.dst_slots(), vec![0, 1]);

    let arith = Instruction::Arith {
        dst: 5,
        op: ArithOp::Add,
        lhs: ValueExpr::Slot(0),
        rhs: ValueExpr::Slot(1),
    };
    assert_eq!(arith.dst_slots(), vec![5]);

    let divmod = Instruction::DivMod {
        dst_q: 3,
        dst_r: 4,
        lhs: ValueExpr::Slot(0),
        rhs: ValueExpr::Slot(1),
    };
    assert_eq!(divmod.dst_slots(), vec![3, 4]);

    let write = Instruction::Write {
        table: TableId(1),
        col: ColId(0),
        row: RowExpr::Literal(RowKey(0)),
        src_val: ValueExpr::Slot(0),
        src_is_null: ValueExpr::Literal(Value::Bool(false)),
    };
    assert_eq!(write.dst_slots(), Vec::<Slot>::new());
}
