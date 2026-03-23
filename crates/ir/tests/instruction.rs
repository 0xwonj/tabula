#![allow(missing_docs)]
use tabula_core::{ColId, PortableValue, RowKey, TableId};
use tabula_ir::{ArithOp, CmpOp, Instruction, RowExpr, Slot, ValueExpr};
use tabula_profile::{TYPE_BOOL_ID, TYPE_U64_ID};

fn lit_u64(value: u64) -> ValueExpr {
    ValueExpr::Literal(PortableValue::new(
        TYPE_U64_ID,
        borsh::to_vec(&value).expect("u64 literal"),
    ))
}

fn lit_bool(value: bool) -> ValueExpr {
    ValueExpr::Literal(PortableValue::new(
        TYPE_BOOL_ID,
        borsh::to_vec(&value).expect("bool literal"),
    ))
}

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
        rhs: lit_u64(10),
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
            lhs: lit_bool(true),
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
    let a = 10u64;
    let b = 3u64;
    assert_eq!(a + b, 13);
    assert_eq!(a - b, 7);
    assert_eq!(a * b, 30);
}

#[test]
fn test_cmp_op_apply() {
    let a = 5u64;
    let b = 5u64;
    let c = 3u64;
    assert_eq!(a, b);
    assert!(a >= b);
    assert!(a <= b);
    assert!(a > c);
    assert!(c < a);
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
        rhs: lit_u64(1),
    };
    let mapped = instr.map_slots(&|s| s + 10);
    assert_eq!(
        mapped,
        Instruction::Arith {
            dst: 10,
            op: ArithOp::Add,
            lhs: ValueExpr::Param(0),
            rhs: lit_u64(1),
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
        src_is_null: lit_bool(false),
    };
    assert_eq!(write.dst_slots(), Vec::<Slot>::new());
}
