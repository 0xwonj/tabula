use tabula_core::SchemeId;
use tabula_machine::SetupError;

use crate::columns::{ColumnPlan, ColumnSchemeFactory, ColumnViews};

/// SMT commitment scheme factory.
pub struct SmtScheme<const W: usize>;

impl<const W: usize> ColumnSchemeFactory for SmtScheme<W> {
    fn scheme_id(&self) -> SchemeId {
        SchemeId::SMT
    }

    fn name(&self) -> &str {
        "smt"
    }

    fn build_column(&self, plan: ColumnPlan) -> Result<ColumnViews, SetupError> {
        let _ = W;
        if plan.scheme_id != SchemeId::SMT {
            return Err(SetupError::SetupFailed(format!(
                "scheme factory '{}' cannot prepare scheme id {}",
                self.name(),
                plan.scheme_id.0,
            )));
        }

        Err(SetupError::SetupFailed(format!(
            "scheme '{}' does not implement proving support for table {} col {}",
            self.name(),
            plan.table_id.0,
            plan.col_id.0,
        )))
    }
}
