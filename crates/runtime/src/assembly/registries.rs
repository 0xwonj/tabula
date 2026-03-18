use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_core::{ColId, TableId};
use tabula_executor::precompile::{PrecompileHandler, PrecompileRegistry};
use tabula_executor::property::{
    CommittedStateProvider, PropertyQueryHandler, PropertyQueryRegistry,
};

use crate::columns::{ColumnPlan, RuntimeColumn};
use crate::error::RuntimeError;

pub(crate) fn build_precompile_registry(
    handlers: Vec<Box<dyn PrecompileHandler>>,
) -> Result<PrecompileRegistry, RuntimeError> {
    let mut registry = PrecompileRegistry::new();
    for handler in handlers {
        registry
            .register_boxed(handler)
            .map_err(|source| RuntimeError::ValidationFailed {
                detail: source.to_string(),
            })?;
    }
    Ok(registry)
}

pub(crate) fn build_property_query_registry(
    runtime_columns: &BTreeMap<(TableId, ColId), Arc<dyn RuntimeColumn>>,
    column_plans: &BTreeMap<(TableId, ColId), ColumnPlan>,
) -> Result<PropertyQueryRegistry, RuntimeError> {
    let mut registry = PropertyQueryRegistry::new();
    for (&(table_id, col_id), plan) in column_plans {
        if !plan.requires_property_support() {
            continue;
        }
        let Some(column) = runtime_columns.get(&(table_id, col_id)).cloned() else {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "missing runtime column for table {} col {} while building property registry",
                    table_id.0, col_id.0,
                ),
            });
        };
        registry
            .register(
                table_id,
                col_id,
                Box::new(ColumnPropertyHandler {
                    table_id,
                    col_id,
                    column,
                }),
            )
            .map_err(|source| RuntimeError::ValidationFailed {
                detail: source.to_string(),
            })?;
    }

    Ok(registry)
}

#[derive(Clone)]
struct ColumnPropertyHandler {
    table_id: TableId,
    col_id: ColId,
    column: Arc<dyn RuntimeColumn>,
}

impl PropertyQueryHandler for ColumnPropertyHandler {
    fn resolve(
        &self,
        query: &tabula_ir::PropertyQuery,
        provider: &dyn CommittedStateProvider,
    ) -> Result<tabula_core::PropertyQueryResult, tabula_core::error::TabulaError> {
        let rows = provider.get_column(self.table_id, self.col_id)?;
        self.column.resolve_property(query, &rows)
    }
}
