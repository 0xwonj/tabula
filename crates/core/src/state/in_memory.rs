//! In-memory implementations of state traits.

use std::collections::{BTreeMap, BTreeSet};

use crate::error::TabulaError;
use crate::traits::{StateSnapshot, StaticTableProvider};
use crate::{CellKey, ColId, RowKey, TableId, Value};

// ---------------------------------------------------------------------------
// InMemoryState
// ---------------------------------------------------------------------------

/// BTreeMap-backed [`StateSnapshot`].
#[derive(Debug, Clone)]
pub struct InMemoryState {
    data: BTreeMap<CellKey, Value>,
    tables: BTreeSet<TableId>,
}

impl InMemoryState {
    /// Create a new empty state.
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
            tables: BTreeSet::new(),
        }
    }

    /// Set a cell value.
    pub fn set(&mut self, key: CellKey, value: Value) {
        self.tables.insert(key.table);
        self.data.insert(key, value);
    }
}

impl Default for InMemoryState {
    fn default() -> Self {
        Self::new()
    }
}

impl StateSnapshot for InMemoryState {
    fn read(&self, key: &CellKey) -> Result<Option<Value>, TabulaError> {
        Ok(self.data.get(key).copied())
    }

    fn table_exists(&self, table: TableId) -> bool {
        self.tables.contains(&table)
    }
}

// ---------------------------------------------------------------------------
// InMemoryStaticTables
// ---------------------------------------------------------------------------

/// BTreeMap-backed [`StaticTableProvider`].
#[derive(Debug, Clone)]
pub struct InMemoryStaticTables {
    data: BTreeMap<(TableId, RowKey, ColId), Value>,
}

impl InMemoryStaticTables {
    /// Create a new empty static table provider.
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    /// Insert a value into the static tables.
    pub fn insert(&mut self, table: TableId, key: RowKey, col: ColId, value: Value) {
        self.data.insert((table, key, col), value);
    }
}

impl Default for InMemoryStaticTables {
    fn default() -> Self {
        Self::new()
    }
}

impl StaticTableProvider for InMemoryStaticTables {
    fn lookup(&self, table: TableId, key: RowKey, col: ColId) -> Result<Value, TabulaError> {
        self.data
            .get(&(table, key, col))
            .copied()
            .ok_or(TabulaError::CellNotFound(CellKey {
                table,
                col,
                row: key,
            }))
    }

    fn contains(&self, table: TableId, key: RowKey) -> Result<bool, TabulaError> {
        use std::ops::Bound;
        let start = (table, key, ColId(u16::MIN));
        let end = (table, key, ColId(u16::MAX));
        Ok(self
            .data
            .range((Bound::Included(start), Bound::Included(end)))
            .next()
            .is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn in_memory_static_tables_lookup() {
        let mut st = InMemoryStaticTables::new();
        st.insert(TableId(1), RowKey(0), ColId(0), Value::U64(42));

        assert_eq!(
            st.lookup(TableId(1), RowKey(0), ColId(0)).unwrap(),
            Value::U64(42)
        );
        assert!(st.contains(TableId(1), RowKey(0)).unwrap());
        assert!(!st.contains(TableId(1), RowKey(999)).unwrap());
        assert!(st.lookup(TableId(1), RowKey(999), ColId(0)).is_err());
    }

    #[test]
    fn in_memory_state_read_write() {
        let mut state = InMemoryState::new();
        let k = CellKey {
            table: TableId(1),
            col: ColId(0),
            row: RowKey(0),
        };
        state.set(k, Value::U64(100));

        assert_eq!(state.read(&k).unwrap(), Some(Value::U64(100)));
        assert!(state.table_exists(TableId(1)));
        assert!(!state.table_exists(TableId(99)));
    }

    #[test]
    fn in_memory_state_none_for_missing() {
        let state = InMemoryState::new();
        let k = CellKey {
            table: TableId(1),
            col: ColId(0),
            row: RowKey(0),
        };
        assert_eq!(state.read(&k).unwrap(), None);
    }
}
