//! PropertyRead support: committed state provider and scheme-backed query registry.
//!
//! The executor resolves `PropertyRead` instructions against pre-batch committed
//! column state. Runtime preparation supplies a per-column registry whose
//! handlers are typically backed by the column's registered commitment scheme.

use std::collections::BTreeMap;

use tabula_core::error::TabulaError;
use tabula_core::{ColId, PropertyQueryResult, RowKey, TableId, Value};
use tabula_ir::PropertyQuery;

/// Provides access to pre-batch committed column state.
///
/// Implementors supply the executor with column data and commitment
/// digests from the last proven batch. The executor never modifies
/// this state — it is read-only snapshot data.
pub trait CommittedStateProvider: Send + Sync {
    /// Retrieve all entries for a committed column.
    ///
    /// Returns `(row_key, value, is_null)` tuples in key-sorted order.
    fn get_column(
        &self,
        table: TableId,
        col: ColId,
    ) -> Result<Vec<(RowKey, Value, bool)>, TabulaError>;
}

/// Resolves structural property queries for one committed column.
pub trait PropertyQueryHandler: Send + Sync {
    /// Resolve a property query against committed state.
    fn resolve(
        &self,
        query: &PropertyQuery,
        provider: &dyn CommittedStateProvider,
    ) -> Result<PropertyQueryResult, TabulaError>;
}

/// Registry of per-column property query handlers.
///
/// Runtime preparation populates this registry using the compiler-owned
/// `(table, col) -> scheme_id` proof plan. Each routed handler is expected
/// to stay aligned with the column's selected commitment scheme.
pub struct PropertyQueryRegistry {
    handlers: BTreeMap<(TableId, ColId), Box<dyn PropertyQueryHandler>>,
}

impl PropertyQueryRegistry {
    /// Create an empty property query registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            handlers: BTreeMap::new(),
        }
    }

    /// Register one handler for a specific committed column.
    pub fn register(
        &mut self,
        table: TableId,
        col: ColId,
        handler: Box<dyn PropertyQueryHandler>,
    ) -> Result<(), TabulaError> {
        let key = (table, col);
        if self.handlers.insert(key, handler).is_some() {
            return Err(TabulaError::InvalidIr(format!(
                "duplicate PropertyRead handler registered for table {} col {}",
                table.0, col.0
            )));
        }
        Ok(())
    }

    /// Whether a handler was registered for the given committed column.
    pub fn contains(&self, table: TableId, col: ColId) -> bool {
        self.handlers.contains_key(&(table, col))
    }

    /// Whether no property handlers are installed.
    pub fn is_empty(&self) -> bool {
        self.handlers.is_empty()
    }

    /// Resolve a property query against one committed column.
    pub fn resolve(
        &self,
        table: TableId,
        col: ColId,
        query: &PropertyQuery,
        provider: &dyn CommittedStateProvider,
    ) -> Result<PropertyQueryResult, TabulaError> {
        let handler = self.handlers.get(&(table, col)).ok_or_else(|| {
            TabulaError::InvalidIr(format!(
                "PropertyRead encountered for table {} col {} but no handler is registered",
                table.0, col.0
            ))
        })?;
        handler.resolve(query, provider)
    }
}

impl Default for PropertyQueryRegistry {
    fn default() -> Self {
        Self::new()
    }
}
