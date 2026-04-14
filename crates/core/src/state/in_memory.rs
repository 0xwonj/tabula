//! In-memory implementations of state traits.

use std::collections::BTreeMap;

use crate::error::TabulaError;
use crate::traits::{StateView, StaticTableProvider};
use crate::{ColId, CommittedCellKey, CommittedKey, PortableValue, RowKey, TableId};

// ---------------------------------------------------------------------------
// InMemoryState
// ---------------------------------------------------------------------------

/// BTreeMap-backed [`StateView`].
#[derive(Debug, Clone)]
pub struct InMemoryState {
    data: BTreeMap<CommittedCellKey, PortableValue>,
}

impl InMemoryState {
    /// Create a new empty state.
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    /// Set a cell value.
    pub fn set(&mut self, key: CommittedCellKey, value: PortableValue) {
        self.data.insert(key, value);
    }
}

impl Default for InMemoryState {
    fn default() -> Self {
        Self::new()
    }
}

impl StateView for InMemoryState {
    fn read(&self, key: &CommittedCellKey) -> Result<Option<PortableValue>, TabulaError> {
        Ok(self.data.get(key).cloned())
    }

    fn column_entries(
        &self,
        table: TableId,
        col: ColId,
    ) -> Result<Vec<(CommittedKey, PortableValue)>, TabulaError> {
        Ok(self
            .data
            .iter()
            .filter(|(key, _)| key.table == table && key.col == col)
            .map(|(key, value)| (key.key.clone(), value.clone()))
            .collect())
    }
}

// ---------------------------------------------------------------------------
// InMemoryStaticTables
// ---------------------------------------------------------------------------

/// BTreeMap-backed [`StaticTableProvider`].
#[derive(Debug, Clone)]
pub struct InMemoryStaticTables {
    data: BTreeMap<(TableId, RowKey, ColId), PortableValue>,
}

impl InMemoryStaticTables {
    /// Create a new empty static table provider.
    pub fn new() -> Self {
        Self {
            data: BTreeMap::new(),
        }
    }

    /// Insert a value into the static tables.
    pub fn insert(&mut self, table: TableId, key: RowKey, col: ColId, value: PortableValue) {
        self.data.insert((table, key, col), value);
    }
}

impl Default for InMemoryStaticTables {
    fn default() -> Self {
        Self::new()
    }
}

impl StaticTableProvider for InMemoryStaticTables {
    fn lookup(
        &self,
        table: TableId,
        key: RowKey,
        col: ColId,
    ) -> Result<PortableValue, TabulaError> {
        self.data
            .get(&(table, key, col))
            .cloned()
            .ok_or(TabulaError::RowNotFound(table, key))
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
    use crate::{PortableValue, TypeId};

    fn portable_u64(value: u64) -> PortableValue {
        PortableValue::new(TypeId(0), value.to_le_bytes().to_vec())
    }

    #[test]
    fn in_memory_static_tables_lookup() {
        let mut st = InMemoryStaticTables::new();
        st.insert(TableId(1), RowKey(0), ColId(0), portable_u64(42));

        assert_eq!(
            st.lookup(TableId(1), RowKey(0), ColId(0)).unwrap(),
            portable_u64(42)
        );
        assert!(st.contains(TableId(1), RowKey(0)).unwrap());
        assert!(!st.contains(TableId(1), RowKey(999)).unwrap());
        assert!(st.lookup(TableId(1), RowKey(999), ColId(0)).is_err());
    }

    #[test]
    fn in_memory_state_read_write() {
        let mut state = InMemoryState::new();
        let k = CommittedCellKey {
            table: TableId(1),
            col: ColId(0),
            key: vec![0].into(),
        };
        state.set(k.clone(), portable_u64(100));

        assert_eq!(state.read(&k).unwrap(), Some(portable_u64(100)));
        assert_eq!(
            state.column_entries(TableId(1), ColId(0)).unwrap(),
            vec![(vec![0].into(), portable_u64(100))]
        );
        assert!(
            state
                .column_entries(TableId(99), ColId(0))
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn in_memory_state_none_for_missing() {
        let state = InMemoryState::new();
        let k = CommittedCellKey {
            table: TableId(1),
            col: ColId(0),
            key: vec![0].into(),
        };
        assert_eq!(state.read(&k).unwrap(), None);
    }
}
