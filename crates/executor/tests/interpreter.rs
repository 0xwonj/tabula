//! Interpreter integration tests — merged from inline tests + extra tests.

mod common;

use tabula_core::error::TabulaError;
use tabula_core::{ColId, RowKey, TableId, Value};
use tabula_ir::{ArithOp, CmpOp, Instruction, RowExpr, ValueExpr};

use common::*;

// ── Arithmetic ──────────────────────────────────────────────────────────

#[test]
fn add_correct() {
    let (_, result) = run(vec![
        Instruction::Arith {
            dst: 0,
            op: ArithOp::Add,
            lhs: ValueExpr::Literal(Value::U64(10)),
            rhs: ValueExpr::Literal(Value::U64(20)),
        },
        write_slot0(),
    ]);
    assert_eq!(
        result.write_set_final,
        vec![(cell(1, 0, 0), Some(Value::U64(30)))]
    );
}

#[test]
fn add_overflow() {
    let err = run_err(vec![Instruction::Arith {
        dst: 0,
        op: ArithOp::Add,
        lhs: ValueExpr::Literal(Value::U64(u64::MAX)),
        rhs: ValueExpr::Literal(Value::U64(1)),
    }]);
    assert_eq!(err.error, TabulaError::ArithmeticOverflow);
}

#[test]
fn sub_correct() {
    let (_, result) = run(vec![
        Instruction::Arith {
            dst: 0,
            op: ArithOp::Sub,
            lhs: ValueExpr::Literal(Value::U64(30)),
            rhs: ValueExpr::Literal(Value::U64(10)),
        },
        write_slot0(),
    ]);
    assert_eq!(
        result.write_set_final,
        vec![(cell(1, 0, 0), Some(Value::U64(20)))]
    );
}

#[test]
fn mul_correct() {
    let (_, result) = run(vec![
        Instruction::Arith {
            dst: 0,
            op: ArithOp::Mul,
            lhs: ValueExpr::Literal(Value::U64(5)),
            rhs: ValueExpr::Literal(Value::U64(7)),
        },
        write_slot0(),
    ]);
    assert_eq!(
        result.write_set_final,
        vec![(cell(1, 0, 0), Some(Value::U64(35)))]
    );
}

#[test]
fn divmod_correct() {
    let (_, result) = run(vec![
        Instruction::DivMod {
            dst_q: 0,
            dst_r: 1,
            lhs: ValueExpr::Literal(Value::U64(17)),
            rhs: ValueExpr::Literal(Value::U64(5)),
        },
        write_slot0(),
        Instruction::Write {
            table: TableId(1),
            row: RowExpr::Literal(RowKey(1)),
            col: ColId(0),
            src_val: ValueExpr::Slot(1),
            src_is_null: ValueExpr::Literal(Value::Bool(false)),
        },
    ]);
    assert!(
        result
            .write_set_final
            .contains(&(cell(1, 0, 0), Some(Value::U64(3))))
    );
    assert!(
        result
            .write_set_final
            .contains(&(cell(1, 1, 0), Some(Value::U64(2))))
    );
}

#[test]
fn divmod_by_zero() {
    let err = run_err(vec![Instruction::DivMod {
        dst_q: 0,
        dst_r: 1,
        lhs: ValueExpr::Literal(Value::U64(10)),
        rhs: ValueExpr::Literal(Value::U64(0)),
    }]);
    assert_eq!(err.error, TabulaError::DivisionByZero);
}

// ── Comparison ──────────────────────────────────────────────────────────

#[test]
fn cmp_eq() {
    run(vec![
        Instruction::Cmp {
            dst: 0,
            op: CmpOp::Eq,
            lhs: ValueExpr::Literal(Value::U64(1)),
            rhs: ValueExpr::Literal(Value::U64(1)),
        },
        Instruction::Assert {
            cond: ValueExpr::Slot(0),
        },
    ]);
}

#[test]
fn cmp_ne_true() {
    run(vec![
        Instruction::Cmp {
            dst: 0,
            op: CmpOp::Ne,
            lhs: ValueExpr::Literal(Value::U64(1)),
            rhs: ValueExpr::Literal(Value::U64(2)),
        },
        Instruction::Assert {
            cond: ValueExpr::Slot(0),
        },
    ]);
}

#[test]
fn cmp_ne_false() {
    let err = run_err(vec![
        Instruction::Cmp {
            dst: 0,
            op: CmpOp::Ne,
            lhs: ValueExpr::Literal(Value::U64(5)),
            rhs: ValueExpr::Literal(Value::U64(5)),
        },
        Instruction::Assert {
            cond: ValueExpr::Slot(0),
        },
    ]);
    assert!(matches!(err.error, TabulaError::AssertionFailed(_)));
}

#[test]
fn cmp_lt() {
    run(vec![
        Instruction::Cmp {
            dst: 0,
            op: CmpOp::Lt,
            lhs: ValueExpr::Literal(Value::U64(1)),
            rhs: ValueExpr::Literal(Value::U64(2)),
        },
        Instruction::Assert {
            cond: ValueExpr::Slot(0),
        },
    ]);
}

#[test]
fn cmp_lte_equal() {
    run(vec![
        Instruction::Cmp {
            dst: 0,
            op: CmpOp::Lte,
            lhs: ValueExpr::Literal(Value::U64(5)),
            rhs: ValueExpr::Literal(Value::U64(5)),
        },
        Instruction::Assert {
            cond: ValueExpr::Slot(0),
        },
    ]);
}

#[test]
fn cmp_lte_less() {
    run(vec![
        Instruction::Cmp {
            dst: 0,
            op: CmpOp::Lte,
            lhs: ValueExpr::Literal(Value::U64(3)),
            rhs: ValueExpr::Literal(Value::U64(5)),
        },
        Instruction::Assert {
            cond: ValueExpr::Slot(0),
        },
    ]);
}

#[test]
fn cmp_lte_greater_fails() {
    let err = run_err(vec![
        Instruction::Cmp {
            dst: 0,
            op: CmpOp::Lte,
            lhs: ValueExpr::Literal(Value::U64(6)),
            rhs: ValueExpr::Literal(Value::U64(5)),
        },
        Instruction::Assert {
            cond: ValueExpr::Slot(0),
        },
    ]);
    assert!(matches!(err.error, TabulaError::AssertionFailed(_)));
}

#[test]
fn cmp_gt_true() {
    run(vec![
        Instruction::Cmp {
            dst: 0,
            op: CmpOp::Gt,
            lhs: ValueExpr::Literal(Value::U64(10)),
            rhs: ValueExpr::Literal(Value::U64(5)),
        },
        Instruction::Assert {
            cond: ValueExpr::Slot(0),
        },
    ]);
}

#[test]
fn cmp_gt_equal_fails() {
    let err = run_err(vec![
        Instruction::Cmp {
            dst: 0,
            op: CmpOp::Gt,
            lhs: ValueExpr::Literal(Value::U64(5)),
            rhs: ValueExpr::Literal(Value::U64(5)),
        },
        Instruction::Assert {
            cond: ValueExpr::Slot(0),
        },
    ]);
    assert!(matches!(err.error, TabulaError::AssertionFailed(_)));
}

#[test]
fn cmp_gte() {
    run(vec![
        Instruction::Cmp {
            dst: 0,
            op: CmpOp::Gte,
            lhs: ValueExpr::Literal(Value::U64(5)),
            rhs: ValueExpr::Literal(Value::U64(5)),
        },
        Instruction::Assert {
            cond: ValueExpr::Slot(0),
        },
    ]);
}

// ── Boolean ops ─────────────────────────────────────────────────────────

#[test]
fn and_or_not() {
    // true AND true → true
    run(vec![
        Instruction::And {
            dst: 0,
            lhs: ValueExpr::Literal(Value::Bool(true)),
            rhs: ValueExpr::Literal(Value::Bool(true)),
        },
        Instruction::Assert {
            cond: ValueExpr::Slot(0),
        },
    ]);

    // true AND false → false; NOT false → true
    run(vec![
        Instruction::And {
            dst: 0,
            lhs: ValueExpr::Literal(Value::Bool(true)),
            rhs: ValueExpr::Literal(Value::Bool(false)),
        },
        Instruction::Not {
            dst: 1,
            src: ValueExpr::Slot(0),
        },
        Instruction::Assert {
            cond: ValueExpr::Slot(1),
        },
    ]);

    // true OR false → true
    run(vec![
        Instruction::Or {
            dst: 0,
            lhs: ValueExpr::Literal(Value::Bool(true)),
            rhs: ValueExpr::Literal(Value::Bool(false)),
        },
        Instruction::Assert {
            cond: ValueExpr::Slot(0),
        },
    ]);
}

#[test]
fn and_lhs_non_bool_fails() {
    let err = run_err(vec![Instruction::And {
        dst: 0,
        lhs: ValueExpr::Literal(Value::U64(1)),
        rhs: ValueExpr::Literal(Value::Bool(true)),
    }]);
    assert!(matches!(
        err.error,
        TabulaError::TypeMismatch {
            expected: "Bool",
            ..
        }
    ));
}

#[test]
fn and_rhs_non_bool_fails() {
    let err = run_err(vec![Instruction::And {
        dst: 0,
        lhs: ValueExpr::Literal(Value::Bool(true)),
        rhs: ValueExpr::Literal(Value::U64(1)),
    }]);
    assert!(matches!(
        err.error,
        TabulaError::TypeMismatch {
            expected: "Bool",
            ..
        }
    ));
}

#[test]
fn or_non_bool_fails() {
    let err = run_err(vec![Instruction::Or {
        dst: 0,
        lhs: ValueExpr::Literal(Value::U64(0)),
        rhs: ValueExpr::Literal(Value::Bool(false)),
    }]);
    assert!(matches!(
        err.error,
        TabulaError::TypeMismatch {
            expected: "Bool",
            ..
        }
    ));
}

#[test]
fn not_non_bool_fails() {
    let err = run_err(vec![Instruction::Not {
        dst: 0,
        src: ValueExpr::Literal(Value::U64(1)),
    }]);
    assert!(matches!(
        err.error,
        TabulaError::TypeMismatch {
            expected: "Bool",
            ..
        }
    ));
}

// ── Assert ──────────────────────────────────────────────────────────────

#[test]
fn assert_passing() {
    run(vec![Instruction::Assert {
        cond: ValueExpr::Literal(Value::Bool(true)),
    }]);
}

#[test]
fn assert_failing() {
    let err = run_err(vec![Instruction::Assert {
        cond: ValueExpr::Literal(Value::Bool(false)),
    }]);
    assert!(matches!(err.error, TabulaError::AssertionFailed(_)));
}

// ── Read / Write ────────────────────────────────────────────────────────

#[test]
fn read_populates_slot() {
    let (out, _) = run_with(
        vec![Instruction::Read {
            dst_val: 0,
            dst_is_null: 1,
            table: TableId(1),
            row: RowExpr::Literal(RowKey(0)),
            col: ColId(0),
        }],
        &[],
        vec![(cell(1, 0, 0), Value::U64(100))],
    );
    assert!(out.emitted.is_empty());
}

#[test]
fn write_updates_overlay() {
    let (_, result) = run(vec![
        Instruction::Write {
            table: TableId(1),
            row: RowExpr::Literal(RowKey(0)),
            col: ColId(0),
            src_val: ValueExpr::Literal(Value::U64(42)),
            src_is_null: ValueExpr::Literal(Value::Bool(false)),
        },
        Instruction::Read {
            dst_val: 0,
            dst_is_null: 1,
            table: TableId(1),
            row: RowExpr::Literal(RowKey(0)),
            col: ColId(0),
        },
    ]);
    assert_eq!(
        result.write_set_final,
        vec![(cell(1, 0, 0), Some(Value::U64(42)))]
    );
}

#[test]
fn write_null_makes_absent() {
    let (_, result) = run_with(
        vec![Instruction::Write {
            table: TableId(1),
            row: RowExpr::Literal(RowKey(0)),
            col: ColId(0),
            src_val: ValueExpr::Literal(Value::U64(0)),
            src_is_null: ValueExpr::Literal(Value::Bool(true)),
        }],
        &[],
        vec![(cell(1, 0, 0), Value::U64(100))],
    );
    assert_eq!(result.write_set_final, vec![(cell(1, 0, 0), None)]);
}

#[test]
fn read_absent_cell_sets_is_null_true() {
    run(vec![
        Instruction::Read {
            dst_val: 0,
            dst_is_null: 1,
            table: TableId(1),
            row: RowExpr::Literal(RowKey(999)),
            col: ColId(0),
        },
        Instruction::Cmp {
            dst: 2,
            op: CmpOp::Eq,
            lhs: ValueExpr::Slot(1),
            rhs: ValueExpr::Literal(Value::Bool(true)),
        },
        Instruction::Assert {
            cond: ValueExpr::Slot(2),
        },
    ]);
}

#[test]
fn write_is_null_from_slot() {
    let result = run(vec![
        Instruction::Read {
            dst_val: 0,
            dst_is_null: 1,
            table: TableId(1),
            row: RowExpr::Literal(RowKey(999)),
            col: ColId(0),
        },
        Instruction::Write {
            table: TableId(1),
            row: RowExpr::Literal(RowKey(0)),
            col: ColId(0),
            src_val: ValueExpr::Literal(Value::U64(0)),
            src_is_null: ValueExpr::Slot(1),
        },
    ])
    .1;
    assert_eq!(result.write_set_final, vec![(cell(1, 0, 0), None)]);
}

#[test]
fn write_is_null_non_bool_fails() {
    let err = run_err(vec![Instruction::Write {
        table: TableId(1),
        row: RowExpr::Literal(RowKey(0)),
        col: ColId(0),
        src_val: ValueExpr::Literal(Value::U64(42)),
        src_is_null: ValueExpr::Literal(Value::U64(0)),
    }]);
    assert!(matches!(
        err.error,
        TabulaError::TypeMismatch {
            expected: "Bool",
            ..
        }
    ));
}

#[test]
fn read_with_param_row() {
    let (_, result) = run_with(
        vec![Instruction::Read {
            dst_val: 0,
            dst_is_null: 1,
            table: TableId(1),
            row: RowExpr::Param(0),
            col: ColId(0),
        }],
        &[Value::U64(5)],
        vec![(cell(1, 5, 0), Value::U64(77))],
    );
    assert_eq!(
        result.read_set_old,
        vec![(cell(1, 5, 0), Some(Value::U64(77)))]
    );
}

// ── Hash ────────────────────────────────────────────────────────────────

#[test]
fn hash_produces_bytes32() {
    let (_, result) = run(vec![
        Instruction::Hash {
            dst: 0,
            inputs: vec![ValueExpr::Literal(Value::U64(42))],
        },
        write_slot0(),
    ]);
    let v = result.write_set_final[0].1.unwrap();
    assert!(matches!(v, Value::Bytes32(_)));
}

#[test]
fn hash_multiple_inputs() {
    let result = run(vec![
        Instruction::Hash {
            dst: 0,
            inputs: vec![
                ValueExpr::Literal(Value::U64(1)),
                ValueExpr::Literal(Value::U64(2)),
                ValueExpr::Literal(Value::U64(3)),
            ],
        },
        write_slot0(),
    ])
    .1;
    let v = result.write_set_final[0].1.unwrap();
    assert!(matches!(v, Value::Bytes32(_)));
}

#[test]
fn hash_empty_inputs() {
    let result = run(vec![
        Instruction::Hash {
            dst: 0,
            inputs: vec![],
        },
        write_slot0(),
    ])
    .1;
    let v = result.write_set_final[0].1.unwrap();
    assert!(matches!(v, Value::Bytes32(_)));
}

// ── Emit ────────────────────────────────────────────────────────────────

#[test]
fn emit_captures_event() {
    let (out, _) = run(vec![Instruction::Emit {
        topic: b"transfer".to_vec(),
        data: vec![ValueExpr::Literal(Value::U64(100))],
    }]);
    assert_eq!(out.emitted.len(), 1);
    assert_eq!(out.emitted[0].topic, b"transfer");
}

// ── Lookup ──────────────────────────────────────────────────────────────

#[test]
fn lookup_delegates() {
    let (_, result) = run(vec![
        Instruction::Lookup {
            dst: 0,
            static_table: TableId(99),
            col: ColId(0),
            row: RowExpr::Literal(RowKey(7)),
        },
        write_slot0(),
    ]);
    assert_eq!(
        result.write_set_final,
        vec![(cell(1, 0, 0), Some(Value::U64(7)))]
    );
}

// ── Select ──────────────────────────────────────────────────────────────

#[test]
fn select_true_branch() {
    let (_, result) = run(vec![
        Instruction::Select {
            dst: 0,
            cond: ValueExpr::Literal(Value::Bool(true)),
            if_true: ValueExpr::Literal(Value::U64(10)),
            if_false: ValueExpr::Literal(Value::U64(20)),
        },
        write_slot0(),
    ]);
    assert_eq!(
        result.write_set_final,
        vec![(cell(1, 0, 0), Some(Value::U64(10)))]
    );
}

#[test]
fn select_false_branch() {
    let (_, result) = run(vec![
        Instruction::Select {
            dst: 0,
            cond: ValueExpr::Literal(Value::Bool(false)),
            if_true: ValueExpr::Literal(Value::U64(10)),
            if_false: ValueExpr::Literal(Value::U64(20)),
        },
        write_slot0(),
    ]);
    assert_eq!(
        result.write_set_final,
        vec![(cell(1, 0, 0), Some(Value::U64(20)))]
    );
}

#[test]
fn select_non_bool_cond_fails() {
    let err = run_err(vec![Instruction::Select {
        dst: 0,
        cond: ValueExpr::Literal(Value::U64(1)),
        if_true: ValueExpr::Literal(Value::U64(10)),
        if_false: ValueExpr::Literal(Value::U64(20)),
    }]);
    assert!(matches!(err.error, TabulaError::TypeMismatch { .. }));
}

// ── Error paths ─────────────────────────────────────────────────────────

#[test]
fn slot_out_of_bounds() {
    let err = run_err(vec![Instruction::Arith {
        dst: 0,
        op: ArithOp::Add,
        lhs: ValueExpr::Slot(5),
        rhs: ValueExpr::Literal(Value::U64(1)),
    }]);
    assert!(matches!(err.error, TabulaError::SlotOutOfBounds { .. }));
}

#[test]
fn param_out_of_bounds() {
    let err = run_err(vec![Instruction::Arith {
        dst: 0,
        op: ArithOp::Add,
        lhs: ValueExpr::Param(10),
        rhs: ValueExpr::Literal(Value::U64(1)),
    }]);
    assert!(matches!(err.error, TabulaError::ParamOutOfBounds { .. }));
}

#[test]
fn slot_gap_error() {
    let err = run_err(vec![
        Instruction::Arith {
            dst: 0,
            op: ArithOp::Add,
            lhs: ValueExpr::Literal(Value::U64(1)),
            rhs: ValueExpr::Literal(Value::U64(1)),
        },
        Instruction::Arith {
            dst: 2,
            op: ArithOp::Add,
            lhs: ValueExpr::Literal(Value::U64(1)),
            rhs: ValueExpr::Literal(Value::U64(1)),
        },
    ]);
    assert!(matches!(err.error, TabulaError::InvalidIr(_)));
    assert_eq!(err.instruction_index, 1);
}

#[test]
fn read_table_not_found() {
    let err = run_err(vec![Instruction::Read {
        dst_val: 0,
        dst_is_null: 1,
        table: TableId(99),
        row: RowExpr::Literal(RowKey(0)),
        col: ColId(0),
    }]);
    assert!(matches!(err.error, TabulaError::TableNotFound(TableId(99))));
}

#[test]
fn read_column_not_found() {
    let err = run_err(vec![Instruction::Read {
        dst_val: 0,
        dst_is_null: 1,
        table: TableId(1),
        row: RowExpr::Literal(RowKey(0)),
        col: ColId(5),
    }]);
    assert!(matches!(err.error, TabulaError::InvalidIr(_)));
}

#[test]
fn write_table_not_found() {
    let err = run_err(vec![Instruction::Write {
        table: TableId(99),
        row: RowExpr::Literal(RowKey(0)),
        col: ColId(0),
        src_val: ValueExpr::Literal(Value::U64(1)),
        src_is_null: ValueExpr::Literal(Value::Bool(false)),
    }]);
    assert!(matches!(err.error, TabulaError::TableNotFound(TableId(99))));
}

// ── instruction_index verification ──────────────────────────────────────

#[test]
fn error_index_first_instruction() {
    let err = run_err(vec![Instruction::Assert {
        cond: ValueExpr::Literal(Value::Bool(false)),
    }]);
    assert_eq!(err.instruction_index, 0);
}

#[test]
fn error_index_third_instruction() {
    let err = run_err(vec![
        Instruction::Arith {
            dst: 0,
            op: ArithOp::Add,
            lhs: ValueExpr::Literal(Value::U64(1)),
            rhs: ValueExpr::Literal(Value::U64(2)),
        },
        Instruction::Arith {
            dst: 1,
            op: ArithOp::Add,
            lhs: ValueExpr::Literal(Value::U64(3)),
            rhs: ValueExpr::Literal(Value::U64(4)),
        },
        Instruction::Assert {
            cond: ValueExpr::Literal(Value::Bool(false)),
        },
    ]);
    assert_eq!(err.instruction_index, 2);
}

#[test]
fn error_index_on_overflow() {
    let err = run_err(vec![
        Instruction::Arith {
            dst: 0,
            op: ArithOp::Add,
            lhs: ValueExpr::Literal(Value::U64(1)),
            rhs: ValueExpr::Literal(Value::U64(1)),
        },
        Instruction::Arith {
            dst: 1,
            op: ArithOp::Add,
            lhs: ValueExpr::Literal(Value::U64(u64::MAX)),
            rhs: ValueExpr::Literal(Value::U64(1)),
        },
    ]);
    assert_eq!(err.instruction_index, 1);
    assert_eq!(err.error, TabulaError::ArithmeticOverflow);
}
