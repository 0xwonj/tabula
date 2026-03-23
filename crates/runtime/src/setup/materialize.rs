use std::collections::BTreeMap;
use std::sync::Arc;

use crate::error::RuntimeError;
use crate::host::{PrecompileFactoryMap, SchemeFactoryMap};
use crate::setup::planning::required_property_queries_by_column;
use tabula_artifact::PrecompileDescriptor;
use tabula_compiler::SealedProgram;
use tabula_core::{ColId, RootProfileId, TableId};
#[cfg(feature = "prove")]
use tabula_executor::precompile::PrecompileHandler;
#[cfg(feature = "prove")]
use tabula_ext::RuntimeColumn;
#[cfg(feature = "prove")]
use tabula_ext::backend::precompile::PrecompileProofPreparer;
use tabula_ext::backend::precompile::{PrecompileProofSystem, ResolvedPrecompile};
#[cfg(feature = "prove")]
use tabula_ext::backend::scheme::ColumnProofBackend;
use tabula_ext::{ColumnBackendSetup, MaterializedColumnBackend, PrecompileBackendFactory};
use tabula_ir::GENERIC_EXECUTION_VALUE_WIDTH;
use tabula_profile::ResolvedColumnProfileRef;
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry};

/// Resolved runtime-facing column state for one compiled program.
pub(crate) struct ResolvedRuntimeColumns {
    #[cfg(feature = "prove")]
    pub(crate) runtime_columns: BTreeMap<(TableId, ColId), Arc<dyn RuntimeColumn>>,
    pub(crate) column_backends: BTreeMap<(TableId, ColId), MaterializedColumnBackend>,
}

/// Ordered proof-preparation slot for one materialized backend.
#[cfg(feature = "prove")]
pub(crate) struct ColumnProofRecipe {
    pub(crate) table: TableId,
    pub(crate) col: ColId,
    pub(crate) proof_backend: Arc<dyn ColumnProofBackend>,
}

/// Canonical ordered proof-materialization slot for one sealed precompile descriptor.
pub(crate) struct ResolvedPrecompileVerifierSystem {
    pub(crate) system: Arc<dyn PrecompileProofSystem>,
}

/// Canonical ordered runtime precompile setup for one sealed descriptor.
#[cfg(feature = "prove")]
pub(crate) struct ResolvedPrecompileRuntimeSetup {
    pub(crate) descriptor: PrecompileDescriptor,
    #[cfg(feature = "prove")]
    pub(crate) preparer: Arc<dyn PrecompileProofPreparer>,
    pub(crate) handler: Arc<dyn PrecompileHandler>,
    pub(crate) system: Arc<dyn PrecompileProofSystem>,
}

/// Materialize all runtime/proof backends for one compiled program using installed factories.
pub(crate) fn materialize_column_backends(
    compiled_program: &SealedProgram,
    backend_factories: &SchemeFactoryMap,
    type_runtimes: &TypeRuntimeRegistry,
    encoding_runtimes: &EncodingRuntimeRegistry,
    accepted_root_binding_families: &[RootProfileId],
) -> Result<ResolvedRuntimeColumns, RuntimeError> {
    compiled_program
        .validate_column_profiles()
        .map_err(|detail| RuntimeError::ValidationFailed { detail })?;

    #[cfg(feature = "prove")]
    let mut runtime_columns = BTreeMap::new();
    let mut column_backends = BTreeMap::new();
    let mut required_property_query_kinds = required_property_queries_by_column(compiled_program);

    for schema in compiled_program.table_schemas() {
        for column in &schema.columns {
            let resolved = compiled_program
                .resolve_column_profile(schema.id, column.id)
                .map_err(|detail| RuntimeError::ValidationFailed { detail })?;
            let required = required_property_query_kinds
                .remove(&(schema.id, column.id))
                .unwrap_or_default();
            let scheme_id = resolved.scheme_profile.scheme_family_id;
            let type_runtime = type_runtimes
                .resolve(resolved.type_descriptor.type_id)
                .map_err(|detail| RuntimeError::ValidationFailed {
                    detail: detail.to_string(),
                })?
                .clone();
            let encoding_runtime = encoding_runtimes
                .resolve(resolved.encoding_profile.encoding_profile_id)
                .map_err(|detail| RuntimeError::ValidationFailed {
                    detail: detail.to_string(),
                })?
                .clone();
            let setup = ColumnBackendSetup {
                table_id: schema.id,
                col_id: column.id,
                profile: resolved,
                type_runtime,
                encoding_runtime,
                required_property_query_kinds: &required,
            };
            let Some(factory) = backend_factories.get(&scheme_id) else {
                return Err(RuntimeError::ValidationFailed {
                    detail: format!(
                        "no canonical backend factory registered for scheme id {}",
                        scheme_id.0
                    ),
                });
            };
            let backend = factory
                .materialize_backend(setup)
                .map_err(RuntimeError::from_extension_setup)?;

            validate_materialized_backend(&backend, resolved, accepted_root_binding_families)?;

            let key = (schema.id, column.id);
            #[cfg(feature = "prove")]
            runtime_columns.insert(key, Arc::clone(&backend.runtime_column));
            column_backends.insert(key, backend);
        }
    }

    Ok(ResolvedRuntimeColumns {
        #[cfg(feature = "prove")]
        runtime_columns,
        column_backends,
    })
}

/// Materialize all verifier-visible precompile proof systems for one sealed program.
pub(crate) fn materialize_precompile_verifier_systems(
    manifest: &[PrecompileDescriptor],
    factories: &PrecompileFactoryMap,
) -> Result<Vec<ResolvedPrecompileVerifierSystem>, RuntimeError> {
    let mut slots = Vec::with_capacity(manifest.len());

    for descriptor in manifest {
        let (factory, resolved) = resolve_precompile_factory(descriptor, factories)?;
        let system = factory
            .build_system(&resolved)
            .map_err(RuntimeError::from_extension_setup)?;
        if system.descriptor() != resolved.descriptor {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "precompile system '{}' returned descriptor {:?} but resolved descriptor is {:?}",
                    system.name(),
                    system.descriptor(),
                    resolved.descriptor,
                ),
            });
        }

        slots.push(ResolvedPrecompileVerifierSystem { system });
    }

    Ok(slots)
}

/// Materialize all runtime-visible precompile systems, preparers, and handlers.
#[cfg(feature = "prove")]
pub(crate) fn materialize_precompile_runtime_backends(
    manifest: &[PrecompileDescriptor],
    factories: &PrecompileFactoryMap,
    encoding_runtimes: &EncodingRuntimeRegistry,
) -> Result<Vec<ResolvedPrecompileRuntimeSetup>, RuntimeError> {
    let mut slots = Vec::with_capacity(manifest.len());

    for descriptor in manifest {
        validate_precompile_execution_width(descriptor, encoding_runtimes)?;
        let (factory, resolved) = resolve_precompile_factory(descriptor, factories)?;
        let system = factory
            .build_system(&resolved)
            .map_err(RuntimeError::from_extension_setup)?;
        if system.descriptor() != resolved.descriptor {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "precompile system '{}' returned descriptor {:?} but resolved descriptor is {:?}",
                    system.name(),
                    system.descriptor(),
                    resolved.descriptor,
                ),
            });
        }
        let preparer = factory
            .build_preparer(&resolved)
            .map_err(RuntimeError::from_extension_setup)?;
        if preparer.descriptor() != &resolved.descriptor {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "precompile preparer '{}' returned descriptor {:?} but resolved descriptor is {:?}",
                    preparer.name(),
                    preparer.descriptor(),
                    resolved.descriptor,
                ),
            });
        }
        let handler = factory
            .build_handler(&resolved)
            .map_err(RuntimeError::from_extension_setup)?;
        if handler.id() != resolved.descriptor.precompile_id {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "precompile handler returned id 0x{:04x} but resolved descriptor requires 0x{:04x}",
                    handler.id().0,
                    resolved.descriptor.precompile_id.0,
                ),
            });
        }
        if handler.signature() != &resolved.descriptor.signature {
            return Err(RuntimeError::ValidationFailed {
                detail: format!(
                    "precompile handler 0x{:04x} returned signature {:?} but resolved descriptor requires {:?}",
                    handler.id().0,
                    handler.signature(),
                    resolved.descriptor.signature,
                ),
            });
        }

        slots.push(ResolvedPrecompileRuntimeSetup {
            descriptor: descriptor.clone(),
            preparer,
            handler,
            system,
        });
    }

    Ok(slots)
}

fn resolve_precompile_factory<'a>(
    descriptor: &PrecompileDescriptor,
    factories: &'a PrecompileFactoryMap,
) -> Result<(&'a Arc<dyn PrecompileBackendFactory>, ResolvedPrecompile), RuntimeError> {
    let Some(factory) = factories.get(&descriptor.precompile_id) else {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "no precompile backend registered for id 0x{:04x}",
                descriptor.precompile_id.0,
            ),
        });
    };
    factory
        .validate_descriptor(descriptor)
        .map_err(RuntimeError::from_extension_setup)?;
    Ok((
        factory,
        ResolvedPrecompile {
            descriptor: descriptor.clone(),
        },
    ))
}

fn validate_precompile_execution_width(
    descriptor: &PrecompileDescriptor,
    encoding_runtimes: &EncodingRuntimeRegistry,
) -> Result<(), RuntimeError> {
    for (kind, slots) in [
        ("input", descriptor.signature.inputs.as_slice()),
        ("output", descriptor.signature.outputs.as_slice()),
    ] {
        for (idx, value_profile) in slots.iter().enumerate() {
            let encoding = encoding_runtimes
                .resolve(value_profile.encoding_profile_id)
                .map_err(|detail| RuntimeError::ValidationFailed {
                    detail: detail.to_string(),
                })?;
            if encoding.descriptor().type_id != value_profile.type_id {
                return Err(RuntimeError::ValidationFailed {
                    detail: format!(
                        "precompile 0x{:04x} {kind} {} declares type {} with incompatible encoding profile {} (encoding type {})",
                        descriptor.precompile_id.0,
                        idx,
                        value_profile.type_id.0,
                        value_profile.encoding_profile_id.0,
                        encoding.descriptor().type_id.0,
                    ),
                });
            }
            if encoding.trace_width() > GENERIC_EXECUTION_VALUE_WIDTH {
                return Err(RuntimeError::ValidationFailed {
                    detail: format!(
                        "precompile 0x{:04x} {kind} {} uses execution width {} but the generic execution lane only supports width {}",
                        descriptor.precompile_id.0,
                        idx,
                        encoding.trace_width(),
                        GENERIC_EXECUTION_VALUE_WIDTH,
                    ),
                });
            }
        }
    }
    Ok(())
}

fn validate_materialized_backend(
    backend: &MaterializedColumnBackend,
    resolved: ResolvedColumnProfileRef<'_>,
    accepted_root_binding_families: &[RootProfileId],
) -> Result<(), RuntimeError> {
    if backend.verifier_contract.scheme_id != resolved.scheme_profile.scheme_family_id {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "materialized backend for scheme {} reported verifier contract scheme {}",
                resolved.scheme_profile.scheme_family_id.0, backend.verifier_contract.scheme_id.0
            ),
        });
    }
    if backend.verifier_contract.proof_layout_family != resolved.proof_layout_family() {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "materialized backend proof layout mismatch: profile={} backend={}",
                resolved.proof_layout_family().0,
                backend.verifier_contract.proof_layout_family.0
            ),
        });
    }
    if backend.verifier_contract.verifier_digest_format != resolved.verifier_digest_format() {
        return Err(RuntimeError::ValidationFailed {
            detail: "materialized backend verifier digest format does not match scheme profile"
                .to_string(),
        });
    }
    if backend.root_binding_contract.root_binding_family != resolved.root_binding_family() {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "materialized backend root binding family mismatch: profile={} backend={}",
                resolved.root_binding_family().0,
                backend.root_binding_contract.root_binding_family.0
            ),
        });
    }
    if backend.root_binding_contract.column_profile_hash != resolved.column_profile.profile_hash {
        return Err(RuntimeError::ValidationFailed {
            detail: "materialized backend root binding contract does not match column profile hash"
                .to_string(),
        });
    }
    if backend.root_binding_contract.receives_commitment != resolved.receives_commitment() {
        return Err(RuntimeError::ValidationFailed {
            detail:
                "materialized backend root binding contract does not match column commitment role"
                    .to_string(),
        });
    }
    if !accepted_root_binding_families.contains(&backend.root_binding_contract.root_binding_family)
    {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "root binding family {} is not accepted by the configured root proof backend",
                backend.root_binding_contract.root_binding_family.0
            ),
        });
    }
    Ok(())
}
