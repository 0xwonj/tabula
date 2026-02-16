//! IR canonicalization: automatically fix fixable NF violations.
//!
//! Pipeline: NF-1 read dedup → slot alias rewriting → slot renumbering.

mod nf1_dedup_read;

use std::collections::BTreeMap;

use crate::{Instruction, Slot};

/// Canonicalize an instruction body.
///
/// Currently performs NF-1 read deduplication and slot compaction.
/// Returns a new body with duplicate reads removed and slots renumbered.
pub fn canonicalize(body: Vec<Instruction>) -> Vec<Instruction> {
    let (body, alias_map) = nf1_dedup_read::dedup_reads(body);
    let body = apply_slot_aliases(body, &alias_map);
    renumber_slots(body)
}

// ---------------------------------------------------------------------------
// Slot alias rewriting
// ---------------------------------------------------------------------------

/// Resolve a slot through the alias map (transitive, breaks on self-loops).
fn resolve_alias(alias_map: &BTreeMap<Slot, Slot>, mut slot: Slot) -> Slot {
    while let Some(&target) = alias_map.get(&slot) {
        if target == slot {
            break;
        }
        slot = target;
    }
    slot
}

/// Rewrite all slot references in the instruction body using the alias map.
fn apply_slot_aliases(
    body: Vec<Instruction>,
    alias_map: &BTreeMap<Slot, Slot>,
) -> Vec<Instruction> {
    if alias_map.is_empty() {
        return body;
    }
    body.into_iter()
        .map(|instr| instr.map_slots(&|s| resolve_alias(alias_map, s)))
        .collect()
}

// ---------------------------------------------------------------------------
// Slot renumbering
// ---------------------------------------------------------------------------

/// Renumber slots so they are contiguous starting from 0.
///
/// Collects all defined (destination) slots, sorts them, and builds
/// an old→new mapping. Then rewrites all references.
fn renumber_slots(body: Vec<Instruction>) -> Vec<Instruction> {
    let defined: Vec<Slot> = body.iter().flat_map(|i| i.dst_slots()).collect();

    // Check if already contiguous — skip rewrite if so.
    let is_contiguous = defined.iter().enumerate().all(|(i, &s)| s as usize == i);
    if is_contiguous {
        return body;
    }

    // Build old→new mapping based on definition order.
    let mut rename_map: BTreeMap<Slot, Slot> = BTreeMap::new();
    let mut next: Slot = 0;
    for s in &defined {
        rename_map.entry(*s).or_insert_with(|| {
            let n = next;
            next += 1;
            n
        });
    }

    body.into_iter()
        .map(|instr| instr.map_slots(&|s| rename_map.get(&s).copied().unwrap_or(s)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ArithOp, RowExpr, ValueExpr};
    use tabula_core::{ColId, RowKey, TableId, Value};

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
}
