//! NF-4 alias guard insertion: for write-involved ambiguous row expression pairs
//! on the same `(table, col)`, insert `Cmp(Ne) + Assert` to enforce distinctness
//! at runtime.
//!
//! Read-read ambiguous pairs are safe under SSA (idempotent reads from base state)
//! and are not guarded.

use std::collections::BTreeMap;

use tabula_core::{ColId, TableId};

use crate::pass::{RowRelation, row_relation, row_to_value_expr};
use crate::{CmpOp, Instruction, RowExpr, Slot, ValueExpr};

/// A state access within the instruction body.
struct Access {
    row: RowExpr,
    is_write: bool,
}

/// Insert alias guard assertions for write-involved ambiguous pairs.
///
/// For each unique ambiguous `(RowExpr, RowExpr)` pair where at least one
/// access is a Write, inserts `Cmp(Ne) + Assert` to enforce distinctness.
/// Returns the modified body with guards prepended or inserted at the
/// appropriate position.
pub(crate) fn insert_alias_guards(body: Vec<Instruction>) -> Vec<Instruction> {
    // 1. Collect state accesses grouped by (table, col).
    let mut by_tc: BTreeMap<(TableId, ColId), Vec<Access>> = BTreeMap::new();
    for instr in &body {
        match instr {
            Instruction::Read {
                table, col, row, ..
            } => {
                by_tc.entry((*table, *col)).or_default().push(Access {
                    row: row.clone(),
                    is_write: false,
                });
            }
            Instruction::Write {
                table, col, row, ..
            } => {
                by_tc.entry((*table, *col)).or_default().push(Access {
                    row: row.clone(),
                    is_write: true,
                });
            }
            _ => {}
        }
    }

    // 2. Find unique ambiguous pairs where at least one is a write.
    let mut guard_pairs: Vec<(RowExpr, RowExpr)> = Vec::new();
    for group in by_tc.values() {
        for i in 0..group.len() {
            for j in (i + 1)..group.len() {
                if !group[i].is_write && !group[j].is_write {
                    continue; // read-read: safe, skip
                }
                if row_relation(&group[i].row, &group[j].row) != RowRelation::Ambiguous {
                    continue; // Equal or Distinct: no guard needed
                }
                let pair = normalize_pair(&group[i].row, &group[j].row);
                if !guard_pairs.contains(&pair) {
                    guard_pairs.push(pair);
                }
            }
        }
    }

    if guard_pairs.is_empty() {
        return body;
    }

    // 3. Allocate slots and generate guard instructions.
    let max_slot = body.iter().flat_map(|i| i.dst_slots()).max().unwrap_or(0);
    let mut next_slot: Slot = max_slot + 1;
    let mut guards: Vec<(usize, Instruction, Instruction)> = Vec::new();

    for (row_a, row_b) in &guard_pairs {
        let dst = next_slot;
        next_slot += 1;
        let insert_pos = guard_insert_position(row_a, row_b, &body);
        let cmp = Instruction::Cmp {
            dst,
            op: CmpOp::Ne,
            lhs: row_to_value_expr(row_a),
            rhs: row_to_value_expr(row_b),
        };
        let assert = Instruction::Assert {
            cond: ValueExpr::Slot(dst),
        };
        guards.push((insert_pos, cmp, assert));
    }

    // 4. Insert guards into body (from highest position to lowest to preserve indices).
    guards.sort_by(|a, b| b.0.cmp(&a.0));
    let mut result = body;
    for (pos, cmp, assert) in guards {
        result.insert(pos, assert);
        result.insert(pos, cmp);
    }
    result
}

/// Determine the insertion position for a guard pair.
///
/// - Param/Literal operands: position 0 (always available)
/// - Slot operands: after the slot's definition instruction
fn guard_insert_position(row_a: &RowExpr, row_b: &RowExpr, body: &[Instruction]) -> usize {
    let pos_a = slot_def_position(row_a, body);
    let pos_b = slot_def_position(row_b, body);
    match (pos_a, pos_b) {
        (Some(a), Some(b)) => a.max(b) + 1,
        (Some(a), None) => a + 1,
        (None, Some(b)) => b + 1,
        (None, None) => 0,
    }
}

/// If the row expression references a Slot, find the instruction index that defines it.
fn slot_def_position(row: &RowExpr, body: &[Instruction]) -> Option<usize> {
    let slot = match row {
        RowExpr::Slot(s) => *s,
        _ => return None,
    };
    body.iter()
        .position(|instr| instr.dst_slots().contains(&slot))
}

/// Normalize a pair so (a, b) and (b, a) are treated as identical.
fn normalize_pair(a: &RowExpr, b: &RowExpr) -> (RowExpr, RowExpr) {
    let ka = pair_key(a);
    let kb = pair_key(b);
    if ka <= kb {
        (a.clone(), b.clone())
    } else {
        (b.clone(), a.clone())
    }
}

fn pair_key(r: &RowExpr) -> (u8, u64) {
    match r {
        RowExpr::Literal(rk) => (0, rk.0),
        RowExpr::Param(p) => (1, *p as u64),
        RowExpr::Slot(s) => (2, *s as u64),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tabula_core::RowKey;
    use tabula_core::Value;

    #[test]
    fn test_no_guards_for_read_read() {
        let body = vec![
            Instruction::Read {
                dst_val: 0,
                dst_is_null: 1,
                table: TableId(0),
                col: ColId(0),
                row: RowExpr::Param(0),
            },
            Instruction::Read {
                dst_val: 2,
                dst_is_null: 3,
                table: TableId(0),
                col: ColId(0),
                row: RowExpr::Param(1),
            },
        ];
        let result = insert_alias_guards(body.clone());
        assert_eq!(result.len(), 2, "no guards inserted for read-read pair");
    }

    #[test]
    fn test_guard_inserted_for_write_write() {
        let body = vec![
            Instruction::Write {
                table: TableId(0),
                col: ColId(0),
                row: RowExpr::Param(0),
                src_val: ValueExpr::Literal(Value::U64(1)),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
            Instruction::Write {
                table: TableId(0),
                col: ColId(0),
                row: RowExpr::Param(1),
                src_val: ValueExpr::Literal(Value::U64(2)),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
        ];
        let result = insert_alias_guards(body);
        assert_eq!(result.len(), 4, "Cmp+Assert inserted before 2 writes");
        assert!(matches!(result[0], Instruction::Cmp { op: CmpOp::Ne, .. }));
        assert!(matches!(result[1], Instruction::Assert { .. }));
    }

    #[test]
    fn test_guard_for_read_write_pair() {
        let body = vec![
            Instruction::Read {
                dst_val: 0,
                dst_is_null: 1,
                table: TableId(0),
                col: ColId(0),
                row: RowExpr::Param(0),
            },
            Instruction::Write {
                table: TableId(0),
                col: ColId(0),
                row: RowExpr::Param(1),
                src_val: ValueExpr::Literal(Value::U64(1)),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
        ];
        let result = insert_alias_guards(body);
        assert_eq!(result.len(), 4, "guard for read+write ambiguous pair");
    }

    #[test]
    fn test_no_guard_for_distinct_literals() {
        let body = vec![
            Instruction::Write {
                table: TableId(0),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(0)),
                src_val: ValueExpr::Literal(Value::U64(1)),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
            Instruction::Write {
                table: TableId(0),
                col: ColId(0),
                row: RowExpr::Literal(RowKey(1)),
                src_val: ValueExpr::Literal(Value::U64(2)),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
        ];
        let result = insert_alias_guards(body);
        assert_eq!(result.len(), 2, "no guard for statically distinct rows");
    }

    #[test]
    fn test_dedup_across_table_col_groups() {
        // Same ambiguous pair (Param(0), Param(1)) in two (table, col) groups
        // but same underlying pair → only one guard
        let body = vec![
            Instruction::Write {
                table: TableId(0),
                col: ColId(0),
                row: RowExpr::Param(0),
                src_val: ValueExpr::Literal(Value::U64(1)),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
            Instruction::Write {
                table: TableId(0),
                col: ColId(0),
                row: RowExpr::Param(1),
                src_val: ValueExpr::Literal(Value::U64(2)),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
            Instruction::Write {
                table: TableId(0),
                col: ColId(1),
                row: RowExpr::Param(0),
                src_val: ValueExpr::Literal(Value::U64(3)),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
            Instruction::Write {
                table: TableId(0),
                col: ColId(1),
                row: RowExpr::Param(1),
                src_val: ValueExpr::Literal(Value::U64(4)),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
        ];
        let result = insert_alias_guards(body);
        // 1 unique pair → 2 guard instructions + 4 original = 6
        assert_eq!(result.len(), 6);
    }

    #[test]
    fn test_slot_based_guard_position() {
        let body = vec![
            Instruction::Arith {
                dst: 0,
                op: crate::ArithOp::Add,
                lhs: ValueExpr::Param(0),
                rhs: ValueExpr::Literal(Value::U64(1)),
            },
            Instruction::Write {
                table: TableId(0),
                col: ColId(0),
                row: RowExpr::Slot(0), // depends on slot 0
                src_val: ValueExpr::Literal(Value::U64(1)),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
            Instruction::Write {
                table: TableId(0),
                col: ColId(0),
                row: RowExpr::Param(1),
                src_val: ValueExpr::Literal(Value::U64(2)),
                src_is_null: ValueExpr::Literal(Value::Bool(false)),
            },
        ];
        let result = insert_alias_guards(body);
        // Guard must come after slot 0 definition (index 0), so at index 1
        assert_eq!(result.len(), 5);
        // index 0: Arith (defines slot 0)
        assert!(matches!(result[0], Instruction::Arith { .. }));
        // index 1: Cmp (guard)
        assert!(matches!(result[1], Instruction::Cmp { op: CmpOp::Ne, .. }));
        // index 2: Assert (guard)
        assert!(matches!(result[2], Instruction::Assert { .. }));
    }
}
