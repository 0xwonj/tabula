//! PropertyRead support: committed state provider and opening registry.
//!
//! These traits enable the executor to resolve `PropertyRead` instructions
//! against pre-batch committed column state without introducing crypto
//! dependencies.

use tabula_core::error::TabulaError;
use tabula_core::{ColId, RowKey, TableId, Value, ValueType};
use tabula_ir::PropertyQuery;

/// Result of resolving a `PropertyRead` query.
#[derive(Debug, Clone)]
pub struct PropertyResult {
    /// The result value (e.g., the value at the minimum key).
    pub value: Value,
    /// The key satisfying the property (e.g., the minimum key).
    /// `None` for aggregate queries (Sum, Count).
    pub key: Option<RowKey>,
    /// Whether the result is null (no matching key found).
    pub is_null: bool,
}

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

/// Resolves `PropertyRead` queries by delegating to registered handlers.
///
/// Each handler knows how to answer structural queries (min, max,
/// successor, etc.) for a specific table/column configuration.
pub trait PropertyOpeningResolver: Send + Sync {
    /// Resolve a property query against committed state.
    fn resolve(
        &self,
        table: TableId,
        col: ColId,
        query: &PropertyQuery,
        provider: &dyn CommittedStateProvider,
        col_type: ValueType,
    ) -> Result<PropertyResult, TabulaError>;
}

/// Registry of property opening resolvers.
///
/// Wraps a `PropertyOpeningResolver` impl for use in `ExecContext`.
/// If no resolvers are registered, the registry is `None` in ExecContext
/// and any `PropertyRead` instruction will error.
pub struct PropertyOpeningRegistry {
    resolver: Box<dyn PropertyOpeningResolver>,
}

impl PropertyOpeningRegistry {
    /// Create a new registry wrapping a resolver.
    pub fn new(resolver: Box<dyn PropertyOpeningResolver>) -> Self {
        Self { resolver }
    }

    /// Resolve a property query.
    pub fn resolve(
        &self,
        table: TableId,
        col: ColId,
        query: &PropertyQuery,
        provider: &dyn CommittedStateProvider,
        col_type: ValueType,
    ) -> Result<PropertyResult, TabulaError> {
        self.resolver.resolve(table, col, query, provider, col_type)
    }
}
