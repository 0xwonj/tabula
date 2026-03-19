use std::collections::{BTreeMap, BTreeSet};

use tabula_compiler::CompiledProgram;

use crate::columns::ColumnPlan;
use crate::error::RuntimeError;

/// Convert the compiler-owned proof plan into per-column column plans.
pub(crate) fn derive_column_plans(
    compiled_program: &CompiledProgram,
) -> Result<Vec<ColumnPlan>, RuntimeError> {
    compiled_program
        .validate_column_proof_plan()
        .map_err(|detail| RuntimeError::ValidationFailed { detail })?;

    let mut value_types = BTreeMap::new();
    for schema in compiled_program.table_schemas() {
        for column in &schema.columns {
            value_types.insert((schema.id, column.id), column.value_type);
        }
    }

    let mut required_property_query_kinds: BTreeMap<_, BTreeSet<_>> = BTreeMap::new();
    for requirement in compiled_program.required_property_requirements() {
        required_property_query_kinds
            .entry((requirement.table_id, requirement.col_id))
            .or_default()
            .insert(requirement.query_kind);
    }

    compiled_program
        .column_proof_plan()
        .iter()
        .map(|plan| {
            let Some(value_type) = value_types.get(&(plan.table_id, plan.col_id)).copied() else {
                return Err(RuntimeError::ValidationFailed {
                    detail: format!(
                        "missing schema value type for table {} col {}",
                        plan.table_id.0, plan.col_id.0,
                    ),
                });
            };
            Ok(ColumnPlan {
                table_id: plan.table_id,
                col_id: plan.col_id,
                scheme_id: plan.scheme_id,
                scheme_descriptor: plan.scheme_descriptor.clone(),
                value_type,
                receives_commitment: plan.receives_commitment,
                required_property_query_kinds: required_property_query_kinds
                    .remove(&(plan.table_id, plan.col_id))
                    .unwrap_or_default(),
            })
        })
        .collect()
}
