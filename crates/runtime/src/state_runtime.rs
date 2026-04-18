use std::collections::BTreeMap;
use std::sync::Arc;

use tabula_compiler::RegisteredProgram;
use tabula_core::error::TabulaError;
use tabula_core::{
    ColId, CommittedCellKey, CommittedKey, ProgramExecutionContract, RootProfileId,
    StateColumnContract, StateTableContract, TableId, TypeId,
};
use tabula_ext::scheme::{ColumnBackendSetup, MaterializedColumnBackend};
use tabula_profile::ResolvedColumnProfileRef;
use tabula_types::{
    CommittedColumnEntry, EncodingRuntimeRegistry, NativeKeyPayload, StateRuntimeView,
    TableKeyCodec, TypeRuntimeRegistry, TypedCommittedPropertyQueryResult, TypedValue,
};

use crate::error::RuntimeError;
use crate::host::SchemeFactoryMap;

#[derive(Clone)]
pub(crate) struct ResolvedStateColumn {
    column_index: usize,
    backend: MaterializedColumnBackend,
}

#[derive(Clone)]
pub(crate) struct ResolvedStateTable {
    table_index: usize,
    key_codec: Arc<TableKeyCodec>,
    columns: BTreeMap<ColId, ResolvedStateColumn>,
}

/// Runtime materialization of the sealed state contract.
#[derive(Clone)]
pub(crate) struct ResolvedStateRuntime {
    contract: Arc<ProgramExecutionContract>,
    tables: BTreeMap<TableId, ResolvedStateTable>,
}

struct ColumnBackendMaterializer<'a> {
    backend_factories: &'a SchemeFactoryMap,
    type_runtimes: &'a TypeRuntimeRegistry,
    encoding_runtimes: &'a EncodingRuntimeRegistry,
    accepted_root_binding_families: &'a [RootProfileId],
}

impl ResolvedStateRuntime {
    pub(crate) fn from_registered_program(
        registered_program: &RegisteredProgram,
        backend_factories: &SchemeFactoryMap,
        type_runtimes: &TypeRuntimeRegistry,
        encoding_runtimes: &EncodingRuntimeRegistry,
        accepted_root_binding_families: &[RootProfileId],
    ) -> Result<Self, RuntimeError> {
        let contract = Arc::new(registered_program.execution_contract().clone());
        let profile_catalog = registered_program.profile_catalog();
        let materializer = ColumnBackendMaterializer {
            backend_factories,
            type_runtimes,
            encoding_runtimes,
            accepted_root_binding_families,
        };

        let mut key_codecs = BTreeMap::new();
        for table in &contract.state.tables {
            let codec = Arc::new(
                TableKeyCodec::from_contract(table.id, &table.key, encoding_runtimes).map_err(
                    |error| RuntimeError::ValidationFailed {
                        detail: error.to_string(),
                    },
                )?,
            );
            if key_codecs.insert(table.id, codec).is_some() {
                return Err(RuntimeError::ValidationFailed {
                    detail: format!(
                        "duplicate table-key codec registration for table {}",
                        table.id.0
                    ),
                });
            }
        }

        let mut tables = BTreeMap::new();
        for (table_index, table) in contract.state.tables.iter().enumerate() {
            let key_codec = key_codecs
                .get(&table.id)
                .ok_or_else(|| RuntimeError::ValidationFailed {
                    detail: format!(
                        "missing table-key codec implementation for table {}",
                        table.id.0
                    ),
                })?
                .clone();
            let mut columns = BTreeMap::new();
            for (column_index, column) in table.columns.iter().enumerate() {
                let resolved = profile_catalog
                    .resolve_column_profile(column.column_profile_id)
                    .map_err(|error| RuntimeError::ValidationFailed {
                        detail: error.to_string(),
                    })?;
                let backend = materialize_column_backend(
                    &materializer,
                    table,
                    column,
                    Arc::clone(&key_codec),
                    resolved,
                )?;
                columns.insert(
                    column.id,
                    ResolvedStateColumn {
                        column_index,
                        backend,
                    },
                );
            }
            tables.insert(
                table.id,
                ResolvedStateTable {
                    table_index,
                    key_codec,
                    columns,
                },
            );
        }

        Ok(Self { contract, tables })
    }

    pub(crate) fn table_contract(
        &self,
        table: TableId,
    ) -> Result<&StateTableContract, RuntimeError> {
        let resolved = self
            .tables
            .get(&table)
            .ok_or_else(|| RuntimeError::ValidationFailed {
                detail: format!("unknown state table {}", table.0),
            })?;
        self.contract
            .state
            .tables
            .get(resolved.table_index)
            .ok_or_else(|| RuntimeError::ValidationFailed {
                detail: format!("missing resolved state table contract {}", table.0),
            })
    }

    pub(crate) fn column_contract(
        &self,
        table: TableId,
        col: ColId,
    ) -> Result<&StateColumnContract, RuntimeError> {
        let resolved_table =
            self.tables
                .get(&table)
                .ok_or_else(|| RuntimeError::ValidationFailed {
                    detail: format!("unknown state table {}", table.0),
                })?;
        let resolved_column =
            resolved_table
                .columns
                .get(&col)
                .ok_or_else(|| RuntimeError::ValidationFailed {
                    detail: format!("unknown state column {}.{}", table.0, col.0),
                })?;
        self.contract
            .state
            .tables
            .get(resolved_table.table_index)
            .and_then(|table| table.columns.get(resolved_column.column_index))
            .ok_or_else(|| RuntimeError::ValidationFailed {
                detail: format!(
                    "missing resolved state column contract {}.{}",
                    table.0, col.0
                ),
            })
    }

    pub(crate) fn column_type(&self, table: TableId, col: ColId) -> Result<TypeId, RuntimeError> {
        Ok(self.column_contract(table, col)?.ty)
    }

    pub(crate) fn key_codec(&self, table: TableId) -> Result<&TableKeyCodec, RuntimeError> {
        self.tables
            .get(&table)
            .map(|table| table.key_codec.as_ref())
            .ok_or_else(|| RuntimeError::ValidationFailed {
                detail: format!("unknown state table {}", table.0),
            })
    }

    pub(crate) fn backend(
        &self,
        table: TableId,
        col: ColId,
    ) -> Result<&MaterializedColumnBackend, RuntimeError> {
        self.tables
            .get(&table)
            .and_then(|table| table.columns.get(&col))
            .map(|column| &column.backend)
            .ok_or_else(|| RuntimeError::ValidationFailed {
                detail: format!("unknown state backend {}.{}", table.0, col.0),
            })
    }

    pub(crate) fn backends(&self) -> impl Iterator<Item = &MaterializedColumnBackend> + '_ {
        self.tables
            .values()
            .flat_map(|table| table.columns.values().map(|column| &column.backend))
    }
}

impl StateRuntimeView for ResolvedStateRuntime {
    fn encode_cell_key(
        &self,
        table: tabula_ir::TableId,
        field: tabula_ir::FieldId,
        key: &[TypedValue],
    ) -> Result<CommittedCellKey, TabulaError> {
        Ok(CommittedCellKey {
            table: table.into(),
            col: field.into(),
            key: self
                .key_codec(table.into())
                .map_err(|error| TabulaError::Custom(error.to_string()))?
                .encode_tuple(key)?,
        })
    }

    fn encode_committed_key(
        &self,
        table: tabula_ir::TableId,
        key: &[TypedValue],
    ) -> Result<CommittedKey, TabulaError> {
        self.key_codec(table.into())
            .map_err(|error| TabulaError::Custom(error.to_string()))?
            .encode_tuple(key)
    }

    fn decode_committed_key(
        &self,
        table: tabula_ir::TableId,
        key: &CommittedKey,
    ) -> Result<Vec<TypedValue>, TabulaError> {
        self.key_codec(table.into())
            .map_err(|error| TabulaError::Custom(error.to_string()))?
            .decode_key(key)
    }

    fn encode_key_payload(
        &self,
        table: tabula_ir::TableId,
        key: &CommittedKey,
    ) -> Result<NativeKeyPayload, TabulaError> {
        self.key_codec(table.into())
            .map_err(|error| TabulaError::Custom(error.to_string()))?
            .encode_padded_proof_payload(key)
    }

    fn compare_keys(
        &self,
        table: tabula_ir::TableId,
        lhs: &CommittedKey,
        rhs: &CommittedKey,
    ) -> Result<std::cmp::Ordering, TabulaError> {
        self.key_codec(table.into())
            .map_err(|error| TabulaError::Custom(error.to_string()))?
            .compare(lhs, rhs)
    }

    fn key_component_types(&self, table: tabula_ir::TableId) -> Result<Vec<TypeId>, TabulaError> {
        Ok(self
            .table_contract(table.into())
            .map_err(|error| TabulaError::Custom(error.to_string()))?
            .key
            .components
            .iter()
            .map(|component| component.ty)
            .collect())
    }

    fn column_type(
        &self,
        table: tabula_ir::TableId,
        field: tabula_ir::FieldId,
    ) -> Result<TypeId, TabulaError> {
        ResolvedStateRuntime::column_type(self, table.into(), field.into())
            .map_err(|error| TabulaError::Custom(error.to_string()))
    }

    fn resolve_property(
        &self,
        table: tabula_ir::TableId,
        field: tabula_ir::FieldId,
        query: &tabula_core::CommittedPropertyQuery,
        state: &[CommittedColumnEntry],
    ) -> Result<TypedCommittedPropertyQueryResult, TabulaError> {
        let column_type = ResolvedStateRuntime::column_type(self, table.into(), field.into())
            .map_err(|error| TabulaError::Custom(error.to_string()))?;
        let result = self
            .backend(table.into(), field.into())
            .map_err(|error| TabulaError::Custom(error.to_string()))?
            .runtime_column
            .resolve_property(query, state)?;
        if result.value.type_id() != column_type {
            return Err(TabulaError::InvalidIr(format!(
                "committed column {}.{} yielded value type {} but field type is {}",
                table.0,
                field.0,
                result.value.type_id().0,
                column_type.0
            )));
        }
        Ok(result)
    }
}

fn materialize_column_backend(
    materializer: &ColumnBackendMaterializer<'_>,
    table: &StateTableContract,
    column: &StateColumnContract,
    key_codec: Arc<TableKeyCodec>,
    resolved: ResolvedColumnProfileRef<'_>,
) -> Result<MaterializedColumnBackend, RuntimeError> {
    let scheme_id = resolved.scheme_profile.scheme_family_id;
    let type_runtime = materializer
        .type_runtimes
        .resolve(resolved.type_descriptor.type_id)
        .map_err(|detail| RuntimeError::ValidationFailed {
            detail: detail.to_string(),
        })?
        .clone();
    let encoding_runtime = materializer
        .encoding_runtimes
        .resolve(resolved.encoding_profile.encoding_profile_id)
        .map_err(|detail| RuntimeError::ValidationFailed {
            detail: detail.to_string(),
        })?
        .clone();
    let Some(factory) = materializer.backend_factories.get(&scheme_id) else {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "no canonical backend factory registered for scheme id {}",
                scheme_id.0,
            ),
        });
    };
    let backend = factory
        .materialize_backend(ColumnBackendSetup {
            table,
            column,
            profile: resolved,
            type_runtime,
            encoding_runtime,
            key_codec,
        })
        .map_err(|error| RuntimeError::ValidationFailed {
            detail: error.to_string(),
        })?;
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
                backend.verifier_contract.proof_layout_family.0,
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
                backend.root_binding_contract.root_binding_family.0,
            ),
        });
    }
    if backend.root_binding_contract.column_profile_hash != resolved.column_profile.profile_hash {
        return Err(RuntimeError::ValidationFailed {
            detail: "materialized backend root binding contract does not match column profile hash"
                .to_string(),
        });
    }
    if !materializer
        .accepted_root_binding_families
        .contains(&backend.root_binding_contract.root_binding_family)
    {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "bundled root authority does not support binding family {} for table {} col {}",
                backend.root_binding_contract.root_binding_family.0,
                backend.table_id.0,
                backend.col_id.0,
            ),
        });
    }
    if backend.table_id != table.id || backend.col_id != column.id {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "materialized backend slot mismatch: expected {}.{} but got {}.{}",
                table.id.0, column.id.0, backend.table_id.0, backend.col_id.0
            ),
        });
    }
    if backend.required_property_query_kinds != column.required_property_queries {
        return Err(RuntimeError::ValidationFailed {
            detail: format!(
                "materialized backend property-query set mismatch for {}.{}",
                table.id.0, column.id.0,
            ),
        });
    }
    Ok(backend)
}
