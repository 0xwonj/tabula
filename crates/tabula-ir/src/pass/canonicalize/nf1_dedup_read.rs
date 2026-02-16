//! NF-1 read deduplication: remove duplicate `Read` instructions for the same
//! `(table, col, row)` cell. The second Read's destination slots are aliased
//! to the first Read's slots. Safe because reading is idempotent.

use std::collections::BTreeMap;

use tabula_core::{ColId, TableId};

use crate::{Instruction, RowExpr, Slot};

use crate::pass::{RowRelation, row_relation};

/// A read within a (table, col) group.
struct ReadKey {
    /// Index into the *original* body (before removal).
    instr_idx: usize,
    dst_val: Slot,
    dst_is_null: Slot,
}

/// Remove duplicate Read instructions, returning the filtered body and
/// a slot alias map (alias → canonical).
pub(crate) fn dedup_reads(body: Vec<Instruction>) -> (Vec<Instruction>, BTreeMap<Slot, Slot>) {
    // Collect all Read positions grouped by (table, col).
    let mut reads_by_tc: BTreeMap<(TableId, ColId), Vec<ReadKey>> = BTreeMap::new();
    for (i, instr) in body.iter().enumerate() {
        if let Instruction::Read {
            dst_val,
            dst_is_null,
            table,
            col,
            ..
        } = instr
        {
            reads_by_tc
                .entry((*table, *col))
                .or_default()
                .push(ReadKey {
                    instr_idx: i,
                    dst_val: *dst_val,
                    dst_is_null: *dst_is_null,
                });
        }
    }

    // For each (t,c) group, find pairs where row is provably Equal.
    // Mark the later Read for removal and record slot aliases.
    let mut remove_set: Vec<bool> = vec![false; body.len()];
    let mut alias_map: BTreeMap<Slot, Slot> = BTreeMap::new();

    for group in reads_by_tc.values() {
        for i in 0..group.len() {
            if remove_set[group[i].instr_idx] {
                continue;
            }
            let row_a = read_row(&body[group[i].instr_idx]);
            for j in (i + 1)..group.len() {
                if remove_set[group[j].instr_idx] {
                    continue;
                }
                let row_b = read_row(&body[group[j].instr_idx]);
                if row_relation(row_a, row_b) == RowRelation::Equal {
                    // Duplicate: alias j's slots to i's slots, remove j.
                    remove_set[group[j].instr_idx] = true;
                    alias_map.insert(group[j].dst_val, group[i].dst_val);
                    alias_map.insert(group[j].dst_is_null, group[i].dst_is_null);
                }
            }
        }
    }

    let filtered: Vec<Instruction> = body
        .into_iter()
        .enumerate()
        .filter(|(i, _)| !remove_set[*i])
        .map(|(_, instr)| instr)
        .collect();

    (filtered, alias_map)
}

/// Extract the RowExpr reference from a Read instruction.
/// Extract the RowExpr from a Read instruction.
///
/// Callers must guarantee `instr` is a `Read` variant — enforced by
/// the `reads_by_tc` grouping in `dedup_reads` which only indexes
/// into positions that were identified as `Read` during the initial scan.
fn read_row(instr: &Instruction) -> &RowExpr {
    match instr {
        Instruction::Read { row, .. } => row,
        _ => {
            debug_assert!(false, "read_row called on non-Read instruction");
            unreachable!()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tabula_core::RowKey;

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
        ];
        let (filtered, alias_map) = dedup_reads(body);
        assert_eq!(filtered.len(), 1);
        assert_eq!(alias_map.get(&2), Some(&0));
        assert_eq!(alias_map.get(&3), Some(&1));
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
        let (filtered, alias_map) = dedup_reads(body);
        assert_eq!(filtered.len(), 1);
        assert_eq!(alias_map.len(), 2);
    }

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
        let (filtered, alias_map) = dedup_reads(body);
        assert_eq!(filtered.len(), 2);
        assert!(alias_map.is_empty());
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
        let (filtered, alias_map) = dedup_reads(body);
        assert_eq!(filtered.len(), 2);
        assert!(alias_map.is_empty());
    }
}
