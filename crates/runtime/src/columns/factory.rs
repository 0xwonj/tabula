use tabula_core::SchemeId;
use tabula_machine::SetupError;

use crate::columns::{ColumnPlan, ColumnViews};

/// Registry-facing factory for a column commitment scheme family.
pub trait ColumnSchemeFactory: Send + Sync {
    /// Portable protocol identifier implemented by this factory.
    fn scheme_id(&self) -> SchemeId;

    /// Human-readable name.
    fn name(&self) -> &str;

    /// Build both runtime and proof views for one `(table, col)` pair.
    fn build_column(&self, plan: ColumnPlan) -> Result<ColumnViews, SetupError>;
}
