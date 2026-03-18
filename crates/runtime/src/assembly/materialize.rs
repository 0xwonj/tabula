use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_compiler::CompiledProgram;
use tabula_core::SchemeId;
#[cfg(feature = "prove")]
use tabula_core::{ColId, TableId};
use tabula_machine::ProofColumn;

#[cfg(feature = "prove")]
use crate::columns::{ColumnPlan, ProofInputBuilder};
use crate::columns::ColumnSchemeFactory;
#[cfg(feature = "prove")]
use crate::columns::RuntimeColumn;
use crate::error::RuntimeError;

use super::planning::derive_column_plans;

/// Resolved runtime/proof column views for one compiled program.
pub(crate) struct ResolvedColumnViews {
    #[cfg(feature = "prove")]
    pub(crate) runtime_columns: BTreeMap<(TableId, ColId), Arc<dyn RuntimeColumn>>,
    #[cfg(feature = "prove")]
    pub(crate) column_plans: BTreeMap<(TableId, ColId), ColumnPlan>,
    #[cfg(feature = "prove")]
    pub(crate) proof_input_builders: BTreeMap<(TableId, ColId), Arc<dyn ProofInputBuilder>>,
    pub(crate) proof_columns: Vec<Arc<dyn ProofColumn>>,
}

/// Materialize all column views for one compiled program using installed factories.
pub(crate) fn resolve_column_views_with_factories(
    compiled_program: &CompiledProgram,
    factories: &BTreeMap<SchemeId, Arc<dyn ColumnSchemeFactory>>,
) -> Result<ResolvedColumnViews, RuntimeError> {
    let plans = derive_column_plans(compiled_program)?;
    #[cfg(feature = "prove")]
    let mut runtime_columns = BTreeMap::new();
    #[cfg(feature = "prove")]
    let mut column_plans = BTreeMap::new();
    #[cfg(feature = "prove")]
    let mut proof_input_builders = BTreeMap::new();
    let mut proof_columns = Vec::with_capacity(plans.len());

    for plan in plans {
        let Some(factory) = factories.get(&plan.scheme_id) else {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "no scheme factory registered for id {} (table {} col {})",
                    plan.scheme_id.0, plan.table_id.0, plan.col_id.0,
                ),
            });
        };

        let views = factory
            .build_column(plan.clone())
            .map_err(RuntimeError::MachineSetup)?;
        #[cfg(feature = "prove")]
        let (runtime, proof, proof_input) = views.into_parts();
        #[cfg(not(feature = "prove"))]
        let (_runtime, proof) = views.into_parts();
        #[cfg(feature = "prove")]
        {
            let key = (plan.table_id, plan.col_id);
            runtime_columns.insert(key, runtime);
            column_plans.insert(key, plan);
            proof_input_builders.insert(key, proof_input);
        }
        proof_columns.push(proof);
    }

    Ok(ResolvedColumnViews {
        #[cfg(feature = "prove")]
        runtime_columns,
        #[cfg(feature = "prove")]
        column_plans,
        #[cfg(feature = "prove")]
        proof_input_builders,
        proof_columns,
    })
}

/// Materialize proof-only column views for one compiled program using installed factories.
pub(crate) fn resolve_proof_columns_with_factories(
    compiled_program: &CompiledProgram,
    factories: &BTreeMap<SchemeId, Arc<dyn ColumnSchemeFactory>>,
) -> Result<Vec<Arc<dyn ProofColumn>>, RuntimeError> {
    Ok(resolve_column_views_with_factories(compiled_program, factories)?.proof_columns)
}
