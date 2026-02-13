//! Opening plan: groups ReadSet_old by (tableId, colId) for batched proofs.

use std::collections::BTreeMap;

use tabula_core::types::{CellKey, ColId, RowKey, TableId, Value};

/// A group of rows to open in a single column of a single table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpeningGroup {
    /// The table containing the column.
    pub table: TableId,
    /// The column within the table.
    pub col: ColId,
    /// Sorted, deduplicated row keys to open.
    pub rows: Vec<RowKey>,
}

/// Build an opening plan from the read set.
///
/// Groups entries by (tableId, colId), deduplicates and sorts rows within
/// each group, and returns groups sorted by (tableId, colId).
pub fn build_opening_plan(read_set_old: &[(CellKey, Option<Value>)]) -> Vec<OpeningGroup> {
    let mut groups: BTreeMap<(TableId, ColId), Vec<RowKey>> = BTreeMap::new();
    for (key, _) in read_set_old {
        groups
            .entry((key.table, key.col))
            .or_default()
            .push(key.row);
    }

    groups
        .into_iter()
        .map(|((table, col), mut rows)| {
            rows.sort();
            rows.dedup();
            OpeningGroup { table, col, rows }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ck(t: u32, r: u64, c: u16) -> CellKey {
        CellKey {
            table: TableId(t),
            col: ColId(c),
            row: RowKey(r),
        }
    }

    #[test]
    fn test_grouping() {
        let read_set = vec![
            (ck(1, 0, 0), Some(Value::U64(10))),
            (ck(1, 1, 0), Some(Value::U64(20))),
            (ck(2, 0, 0), Some(Value::U64(30))),
        ];
        let plan = build_opening_plan(&read_set);
        assert_eq!(plan.len(), 2); // table 1 col 0, table 2 col 0
        assert_eq!(plan[0].table, TableId(1));
        assert_eq!(plan[0].rows, vec![RowKey(0), RowKey(1)]);
        assert_eq!(plan[1].table, TableId(2));
        assert_eq!(plan[1].rows, vec![RowKey(0)]);
    }

    #[test]
    fn test_dedup() {
        let read_set = vec![
            (ck(1, 0, 0), Some(Value::U64(10))),
            (ck(1, 0, 0), Some(Value::U64(10))), // duplicate
        ];
        let plan = build_opening_plan(&read_set);
        assert_eq!(plan.len(), 1);
        assert_eq!(plan[0].rows, vec![RowKey(0)]);
    }

    #[test]
    fn test_empty() {
        let plan = build_opening_plan(&[]);
        assert!(plan.is_empty());
    }

    #[test]
    fn test_multi_column() {
        let read_set = vec![
            (ck(1, 0, 0), Some(Value::U64(10))),
            (ck(1, 0, 1), Some(Value::U64(20))),
        ];
        let plan = build_opening_plan(&read_set);
        assert_eq!(plan.len(), 2); // same table, different columns
        assert_eq!(plan[0].col, ColId(0));
        assert_eq!(plan[1].col, ColId(1));
    }
}
