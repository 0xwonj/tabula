use tabula_core::{ColId, RowKey, TableId, Value};
use tabula_ir::pass::canonicalize::canonicalize;
use tabula_ir::{ArithOp, Instruction, RowExpr, ValueExpr};

#[test]
fn test_no_duplicates_unchanged() {
    let body = vec![
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
            row: RowExpr::Literal(RowKey(1)), // different row
        },
    ];
    let result = canonicalize(body.clone());
    assert_eq!(result.len(), 2);
}

#[test]
fn test_dedup_literal_reads() {
    let body = vec![
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
            row: RowExpr::Literal(RowKey(0)), // same cell
        },
        // Use slot 2 (which should become slot 0 after alias + renumber)
        Instruction::Arith {
            dst: 4,
            op: ArithOp::Add,
            lhs: ValueExpr::Slot(2),
            rhs: ValueExpr::Literal(Value::U64(1)),
        },
    ];
    let result = canonicalize(body);
    // Second Read removed → 2 instructions remain.
    assert_eq!(result.len(), 2);

    // The Arith should reference slot 0 (aliased from slot 2).
    match &result[1] {
        Instruction::Arith { lhs, .. } => {
            assert_eq!(*lhs, ValueExpr::Slot(0));
        }
        _ => panic!("expected Arith"),
    }
}

#[test]
fn test_dedup_param_reads() {
    let body = vec![
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
            row: RowExpr::Param(0), // same param
        },
    ];
    let result = canonicalize(body);
    assert_eq!(result.len(), 1);
}

#[test]
fn test_different_tables_not_deduped() {
    let body = vec![
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
            table: TableId(2), // different table
            col: ColId(0),
            row: RowExpr::Literal(RowKey(0)),
        },
    ];
    let result = canonicalize(body);
    assert_eq!(result.len(), 2);
}

#[test]
fn test_slot_renumbering() {
    // Slots 0,1 then 4,5 (gap at 2,3) → should become 0,1,2,3
    let body = vec![
        Instruction::Read {
            dst_val: 0,
            dst_is_null: 1,
            table: TableId(1),
            col: ColId(0),
            row: RowExpr::Literal(RowKey(0)),
        },
        Instruction::Arith {
            dst: 4,
            op: ArithOp::Add,
            lhs: ValueExpr::Slot(0),
            rhs: ValueExpr::Literal(Value::U64(1)),
        },
        Instruction::Arith {
            dst: 5,
            op: ArithOp::Add,
            lhs: ValueExpr::Slot(4),
            rhs: ValueExpr::Literal(Value::U64(2)),
        },
    ];
    let result = canonicalize(body);
    assert_eq!(result.len(), 3);

    // Slot 4 → 2, Slot 5 → 3
    match &result[1] {
        Instruction::Arith { dst, lhs, .. } => {
            assert_eq!(*dst, 2);
            assert_eq!(*lhs, ValueExpr::Slot(0));
        }
        _ => panic!("expected Arith"),
    }
    match &result[2] {
        Instruction::Arith { dst, lhs, .. } => {
            assert_eq!(*dst, 3);
            assert_eq!(*lhs, ValueExpr::Slot(2));
        }
        _ => panic!("expected Arith"),
    }
}

#[test]
fn test_already_contiguous_no_change() {
    let body = vec![
        Instruction::Read {
            dst_val: 0,
            dst_is_null: 1,
            table: TableId(1),
            col: ColId(0),
            row: RowExpr::Literal(RowKey(0)),
        },
        Instruction::Arith {
            dst: 2,
            op: ArithOp::Add,
            lhs: ValueExpr::Slot(0),
            rhs: ValueExpr::Literal(Value::U64(1)),
        },
    ];
    let result = canonicalize(body.clone());
    assert_eq!(result, body);
}
