use std::collections::BTreeMap;
use std::sync::Arc;

use crate::columns::ResolvedColumnPlan;
#[cfg(feature = "prove")]
use crate::columns::{ColumnSchemeFactory, RuntimeColumn};
use crate::error::RuntimeError;
#[cfg(feature = "prove")]
use crate::precompile_proofs::PrecompileProofPreparer;
use crate::precompile_proofs::{PrecompileProofFactory, PrecompileProofSystem, ResolvedPrecompile};
#[cfg(feature = "prove")]
use crate::proof_extensions::ColumnProofPreparer;
use crate::proof_extensions::ProofSchemeFactory;
use tabula_artifact::PrecompileDescriptor;
#[cfg(feature = "prove")]
use tabula_core::ValueType;
#[cfg(feature = "prove")]
use tabula_core::{ColId, TableId};
use tabula_core::{RootProfileId, SchemeId};
use tabula_ir::PrecompileId;
use tabula_machine::backend::ProofColumn;

/// Resolved execution-facing column state for one compiled program.
#[cfg(feature = "prove")]
pub(crate) struct ResolvedRuntimeColumns {
    pub(crate) runtime_columns: BTreeMap<(TableId, ColId), Arc<dyn RuntimeColumn>>,
    pub(crate) column_plans: BTreeMap<(TableId, ColId), ResolvedColumnPlan>,
}

/// Ordered proof-preparation slot for one materialized column.
#[cfg(feature = "prove")]
pub(crate) struct ColumnProofRecipe {
    pub(crate) table: TableId,
    pub(crate) col: ColId,
    pub(crate) value_type: ValueType,
    pub(crate) preparer: Arc<dyn ColumnProofPreparer>,
}

/// Canonical ordered proof-materialization slot for one derived column plan.
pub(crate) struct ResolvedColumnProofSetup {
    #[cfg(feature = "prove")]
    pub(crate) plan: ResolvedColumnPlan,
    pub(crate) proof_column: Arc<dyn ProofColumn>,
    #[cfg(feature = "prove")]
    pub(crate) preparer: Arc<dyn ColumnProofPreparer>,
}

/// Canonical ordered proof-materialization slot for one sealed precompile descriptor.
pub(crate) struct ResolvedPrecompileProofSetup {
    pub(crate) system: Arc<dyn PrecompileProofSystem>,
    #[cfg(feature = "prove")]
    pub(crate) descriptor: PrecompileDescriptor,
    #[cfg(feature = "prove")]
    pub(crate) preparer: Arc<dyn PrecompileProofPreparer>,
}

/// Materialize all runtime columns for one compiled program using installed factories.
#[cfg(feature = "prove")]
pub(crate) fn resolve_runtime_columns_with_factories(
    plans: &[ResolvedColumnPlan],
    factories: &BTreeMap<SchemeId, Arc<dyn ColumnSchemeFactory>>,
    root_profile_id: RootProfileId,
) -> Result<ResolvedRuntimeColumns, RuntimeError> {
    let mut runtime_columns = BTreeMap::new();
    let mut column_plans = BTreeMap::new();

    for plan in plans {
        let Some(factory) = factories.get(&plan.scheme_id) else {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "no runtime scheme factory registered for id {} (table {} col {})",
                    plan.scheme_id.0, plan.table_id.0, plan.col_id.0,
                ),
            });
        };

        validate_descriptor(&factory.descriptor(), plan, root_profile_id)?;

        let key = (plan.table_id, plan.col_id);
        let runtime = factory
            .build_runtime_column(plan)
            .map_err(RuntimeError::from_extension_setup)?;
        runtime_columns.insert(key, runtime);
        column_plans.insert(key, plan.clone());
    }

    Ok(ResolvedRuntimeColumns {
        runtime_columns,
        column_plans,
    })
}

/// Materialize all proof extensions for one compiled program using installed factories.
pub(crate) fn materialize_proof_slots_with_factories(
    plans: &[ResolvedColumnPlan],
    factories: &BTreeMap<SchemeId, Arc<dyn ProofSchemeFactory>>,
    root_profile_id: RootProfileId,
) -> Result<Vec<ResolvedColumnProofSetup>, RuntimeError> {
    let mut slots = Vec::with_capacity(plans.len());

    for plan in plans {
        let Some(factory) = factories.get(&plan.scheme_id) else {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "no proof scheme factory registered for id {} (table {} col {})",
                    plan.scheme_id.0, plan.table_id.0, plan.col_id.0,
                ),
            });
        };

        validate_descriptor(&factory.descriptor(), plan, root_profile_id)?;

        let proof_column = factory
            .build_proof_column(plan)
            .map_err(RuntimeError::from_extension_setup)?;
        #[cfg(feature = "prove")]
        let preparer = factory
            .build_proof_preparer(plan)
            .map_err(RuntimeError::from_extension_setup)?;

        slots.push(ResolvedColumnProofSetup {
            #[cfg(feature = "prove")]
            plan: plan.clone(),
            proof_column,
            #[cfg(feature = "prove")]
            preparer,
        });
    }

    Ok(slots)
}

/// Materialize all precompile proof systems for one sealed program.
pub(crate) fn materialize_precompile_proofs_with_factories(
    manifest: &[PrecompileDescriptor],
    factories: &BTreeMap<PrecompileId, Arc<dyn PrecompileProofFactory>>,
) -> Result<Vec<ResolvedPrecompileProofSetup>, RuntimeError> {
    let mut slots = Vec::with_capacity(manifest.len());

    for descriptor in manifest {
        let Some(factory) = factories.get(&descriptor.precompile_id) else {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "no precompile proof factory registered for id 0x{:04x}",
                    descriptor.precompile_id.0,
                ),
            });
        };

        let factory_descriptor = factory.descriptor();
        if &factory_descriptor != descriptor {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "precompile descriptor mismatch for id 0x{:04x}: artifact={:?} factory={:?}",
                    descriptor.precompile_id.0, descriptor, factory_descriptor,
                ),
            });
        }

        let resolved = ResolvedPrecompile {
            descriptor: descriptor.clone(),
        };
        let system = factory
            .build_system(&resolved)
            .map_err(RuntimeError::from_extension_setup)?;
        #[cfg(feature = "prove")]
        let preparer = factory
            .build_preparer(&resolved)
            .map_err(RuntimeError::from_extension_setup)?;

        slots.push(ResolvedPrecompileProofSetup {
            system,
            #[cfg(feature = "prove")]
            descriptor: descriptor.clone(),
            #[cfg(feature = "prove")]
            preparer,
        });
    }

    Ok(slots)
}

fn validate_descriptor(
    descriptor: &tabula_artifact::SchemeDescriptor,
    plan: &ResolvedColumnPlan,
    root_profile_id: RootProfileId,
) -> Result<(), RuntimeError> {
    if descriptor != &plan.scheme_descriptor {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "scheme descriptor mismatch for table {} col {}: artifact={:?} factory={:?}",
                plan.table_id.0, plan.col_id.0, plan.scheme_descriptor, descriptor,
            ),
        });
    }
    if descriptor.root_profile_id != root_profile_id {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "scheme descriptor for table {} col {} requires root profile {} but runtime/verifier is configured for {}",
                plan.table_id.0, plan.col_id.0, descriptor.root_profile_id.0, root_profile_id.0,
            ),
        });
    }
    Ok(())
}
