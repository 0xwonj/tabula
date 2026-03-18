use tabula_core::error::TabulaError;
use tabula_core::{PropertyQueryResult, RowKey, Value};
use tabula_ir::{PropertyQuery, PropertyQueryKind};

/// Execution-facing per-column view.
pub trait RuntimeColumn: Send + Sync {
    /// Human-readable scheme name.
    fn name(&self) -> &str;

    /// Structural property queries this column supports.
    fn supported_property_query_kinds(&self) -> &[PropertyQueryKind] {
        &[]
    }

    /// Resolve a structural property query over one committed column snapshot.
    fn resolve_property(
        &self,
        query: &PropertyQuery,
        state: &[(RowKey, Value, bool)],
    ) -> Result<PropertyQueryResult, TabulaError> {
        let _ = state;
        Err(TabulaError::InvalidIr(format!(
            "column scheme '{}' does not support property query {:?}",
            self.name(),
            query.kind(),
        )))
    }
}
