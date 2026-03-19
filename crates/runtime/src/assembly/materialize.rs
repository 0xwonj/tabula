use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_compiler::CompiledProgram;
#[cfg(feature = "prove")]
use tabula_core::{ColId, TableId};
use tabula_core::{RootProfileId, SchemeId};
use tabula_machine::ProofColumn;

use crate::columns::ColumnSchemeFactory;
#[cfg(feature = "prove")]
use crate::columns::{ColumnPlan, ColumnTransitionBackend, RuntimeColumn};
use crate::error::RuntimeError;

use super::planning::derive_column_plans;

/// Resolved runtime/proof column views for one compiled program.
pub(crate) struct ResolvedColumnViews {
    #[cfg(feature = "prove")]
    pub(crate) runtime_columns: BTreeMap<(TableId, ColId), Arc<dyn RuntimeColumn>>,
    #[cfg(feature = "prove")]
    pub(crate) column_plans: BTreeMap<(TableId, ColId), ColumnPlan>,
    #[cfg(feature = "prove")]
    pub(crate) transition_backends: BTreeMap<(TableId, ColId), Arc<dyn ColumnTransitionBackend>>,
    pub(crate) proof_columns: Vec<Arc<dyn ProofColumn>>,
}

/// Materialize all column views for one compiled program using installed factories.
pub(crate) fn resolve_column_views_with_factories(
    compiled_program: &CompiledProgram,
    factories: &BTreeMap<SchemeId, Arc<dyn ColumnSchemeFactory>>,
    root_profile_id: RootProfileId,
) -> Result<ResolvedColumnViews, RuntimeError> {
    let plans = derive_column_plans(compiled_program)?;
    #[cfg(feature = "prove")]
    let mut runtime_columns = BTreeMap::new();
    #[cfg(feature = "prove")]
    let mut column_plans = BTreeMap::new();
    #[cfg(feature = "prove")]
    let mut transition_backends = BTreeMap::new();
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

        let descriptor = factory.descriptor();
        if descriptor != plan.scheme_descriptor {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "scheme descriptor mismatch for table {} col {}: artifact={:?} factory={:?}",
                    plan.table_id.0, plan.col_id.0, plan.scheme_descriptor, descriptor,
                ),
            });
        }
        if plan.scheme_descriptor.root_profile_id != root_profile_id {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "scheme descriptor for table {} col {} requires root profile {} but runtime/verifier is configured for {}",
                    plan.table_id.0,
                    plan.col_id.0,
                    plan.scheme_descriptor.root_profile_id.0,
                    root_profile_id.0,
                ),
            });
        }

        let views = factory
            .build_column(plan.clone())
            .map_err(RuntimeError::MachineSetup)?;
        #[cfg(feature = "prove")]
        let (runtime, proof, transition) = views.into_parts();
        #[cfg(not(feature = "prove"))]
        let (_runtime, proof) = views.into_parts();
        #[cfg(feature = "prove")]
        {
            let key = (plan.table_id, plan.col_id);
            runtime_columns.insert(key, runtime);
            column_plans.insert(key, plan.clone());
            transition_backends.insert(key, transition);
        }
        proof_columns.push(proof);
    }

    Ok(ResolvedColumnViews {
        #[cfg(feature = "prove")]
        runtime_columns,
        #[cfg(feature = "prove")]
        column_plans,
        #[cfg(feature = "prove")]
        transition_backends,
        proof_columns,
    })
}

/// Materialize proof-only column views for one compiled program using installed factories.
pub(crate) fn resolve_proof_columns_with_factories(
    compiled_program: &CompiledProgram,
    factories: &BTreeMap<SchemeId, Arc<dyn ColumnSchemeFactory>>,
    root_profile_id: RootProfileId,
) -> Result<Vec<Arc<dyn ProofColumn>>, RuntimeError> {
    Ok(
        resolve_column_views_with_factories(compiled_program, factories, root_profile_id)?
            .proof_columns,
    )
}
