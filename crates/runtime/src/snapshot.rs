//! Committed state snapshot input for the native runtime.

use std::collections::BTreeMap;

use borsh::BorshSerialize;
use sha2::Digest as _;
use tabula_core::error::TabulaError;
use tabula_core::traits::StateView;
use tabula_core::{ColId, CommittedCellKey, CommittedKey, Digest, PortableValue, TableId};
use tabula_ir as ir;
#[cfg(feature = "prove")]
use tabula_types::CommittedColumnEntry;
use tabula_types::TypeRuntimeRegistry;
#[cfg(feature = "prove")]
use tabula_witness::CommittedEntry;

use crate::error::SetupError;
use crate::state_runtime::ResolvedStateRuntime;

pub(crate) type LogicalStateCell = (ir::TableId, Vec<PortableValue>, ir::FieldId, PortableValue);

#[derive(Debug, Clone, PartialEq, Eq, BorshSerialize)]
struct SnapshotCellRecord {
    key: CommittedCellKey,
    value: PortableValue,
}

/// Proof-capable committed state input for the native runtime.
#[non_exhaustive]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommittedStateSnapshot {
    cells: BTreeMap<CommittedCellKey, PortableValue>,
}

impl CommittedStateSnapshot {
    /// Create an empty committed snapshot.
    pub(crate) fn empty() -> Self {
        Self {
            cells: BTreeMap::new(),
        }
    }

    /// Build one committed snapshot from logical key tuples.
    pub(crate) fn from_cells<I>(
        state_runtime: &ResolvedStateRuntime,
        type_runtimes: &TypeRuntimeRegistry,
        cells: I,
    ) -> Result<Self, SetupError>
    where
        I: IntoIterator<Item = (ir::TableId, Vec<PortableValue>, ir::FieldId, PortableValue)>,
    {
        let mut snapshot = Self::empty();
        for (table, key, field, value) in cells {
            snapshot.insert(state_runtime, type_runtimes, table, &key, field, value)?;
        }
        Ok(snapshot)
    }

    pub(crate) fn from_committed_cells<I>(
        state_runtime: &ResolvedStateRuntime,
        type_runtimes: &TypeRuntimeRegistry,
        cells: I,
    ) -> Result<Self, SetupError>
    where
        I: IntoIterator<Item = (ir::TableId, Vec<u8>, ir::FieldId, PortableValue)>,
    {
        let mut snapshot = Self::empty();
        for (table, key, field, value) in cells {
            let cell_key = CommittedCellKey {
                table: table.into(),
                col: field.into(),
                key: CommittedKey(key),
            };
            if snapshot.cells.contains_key(&cell_key) {
                return Err(SetupError::Validation {
                    detail: format!(
                        "duplicate committed cell {}.{} key {} in external snapshot payload",
                        cell_key.table.0, cell_key.col.0, cell_key.key
                    ),
                });
            }
            snapshot.insert_materialized(cell_key, value);
        }
        snapshot.validate(state_runtime, type_runtimes)?;
        Ok(snapshot)
    }

    /// Insert one committed cell after validating it against the sealed state contract.
    pub(crate) fn insert(
        &mut self,
        state_runtime: &ResolvedStateRuntime,
        type_runtimes: &TypeRuntimeRegistry,
        table: ir::TableId,
        key: &[PortableValue],
        field: ir::FieldId,
        value: PortableValue,
    ) -> Result<(), SetupError> {
        let field_schema = state_runtime
            .column_contract(table.into(), field.into())
            .map_err(|error| SetupError::Validation {
                detail: error.to_string(),
            })?;
        let table_key_codec =
            state_runtime
                .key_codec(table.into())
                .map_err(|error| SetupError::Validation {
                    detail: error.to_string(),
                })?;
        let typed_key = key
            .iter()
            .map(|value| type_runtimes.decode_portable(value))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| SetupError::Validation {
                detail: error.to_string(),
            })?;
        let committed_key =
            table_key_codec
                .encode_tuple(&typed_key)
                .map_err(|error| SetupError::Validation {
                    detail: error.to_string(),
                })?;
        if value.type_id() != field_schema.ty {
            return Err(SetupError::Validation {
                detail: format!(
                    "state cell {}.{} key {} stores type {} but field expects {}",
                    table.0,
                    field.0,
                    committed_key,
                    value.type_id().0,
                    field_schema.ty.0,
                ),
            });
        }
        let cell_key = CommittedCellKey {
            table: table.into(),
            col: field.into(),
            key: committed_key,
        };
        if self.cells.contains_key(&cell_key) {
            return Err(SetupError::Validation {
                detail: format!(
                    "duplicate logical state cell {}.{} key {} in external state payload",
                    cell_key.table.0, cell_key.col.0, cell_key.key
                ),
            });
        }
        self.cells.insert(cell_key, value);
        Ok(())
    }

    pub(crate) fn insert_materialized(&mut self, key: CommittedCellKey, value: PortableValue) {
        self.cells.insert(key, value);
    }

    pub(crate) fn remove_materialized(
        &mut self,
        table: ir::TableId,
        key: &CommittedKey,
        field: ir::FieldId,
    ) {
        self.cells.remove(&CommittedCellKey {
            table: table.into(),
            col: field.into(),
            key: key.clone(),
        });
    }

    pub(crate) fn validate(
        &self,
        state_runtime: &ResolvedStateRuntime,
        type_runtimes: &TypeRuntimeRegistry,
    ) -> Result<(), SetupError> {
        for (key, value) in &self.cells {
            let column = state_runtime
                .column_contract(key.table, key.col)
                .map_err(|error| SetupError::Validation {
                    detail: error.to_string(),
                })?;
            if value.type_id() != column.ty {
                return Err(SetupError::Validation {
                    detail: format!(
                        "committed cell {}.{} key {} stores type {} but field expects {}",
                        key.table.0,
                        key.col.0,
                        key.key,
                        value.type_id().0,
                        column.ty.0,
                    ),
                });
            }
            let table_key_codec =
                state_runtime
                    .key_codec(key.table)
                    .map_err(|error| SetupError::Validation {
                        detail: error.to_string(),
                    })?;
            let decoded =
                table_key_codec
                    .decode_key(&key.key)
                    .map_err(|error| SetupError::Validation {
                        detail: error.to_string(),
                    })?;
            let reencoded =
                table_key_codec
                    .encode_tuple(&decoded)
                    .map_err(|error| SetupError::Validation {
                        detail: error.to_string(),
                    })?;
            if reencoded != key.key {
                return Err(SetupError::Validation {
                    detail: format!(
                        "committed cell {}.{} key {} is not canonical",
                        key.table.0, key.col.0, key.key
                    ),
                });
            }
            type_runtimes
                .decode_portable(value)
                .map_err(|error| SetupError::Validation {
                    detail: error.to_string(),
                })?;
        }
        Ok(())
    }

    /// Iterate committed cells in canonical `(table, col, committed_key)` order.
    pub fn cells(&self) -> impl Iterator<Item = (&CommittedCellKey, &PortableValue)> {
        self.cells.iter()
    }

    /// Serialize the snapshot canonically for transcript or external binding.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, SetupError> {
        let records = self
            .cells
            .iter()
            .map(|(key, value)| SnapshotCellRecord {
                key: key.clone(),
                value: value.clone(),
            })
            .collect::<Vec<_>>();
        let mut bytes = b"tabula.runtime.committed_state_snapshot.v1".to_vec();
        bytes.extend(
            borsh::to_vec(&records).map_err(|error| SetupError::Validation {
                detail: format!("failed to encode state snapshot: {error}"),
            })?,
        );
        Ok(bytes)
    }

    /// Canonical digest of the committed state snapshot.
    pub fn canonical_digest(&self) -> Result<Digest, SetupError> {
        let bytes = self.canonical_bytes()?;
        Ok(sha2::Sha256::digest(bytes).into())
    }

    #[cfg(feature = "prove")]
    fn committed_column_entries(
        &self,
        table: TableId,
        col: ColId,
        type_runtimes: &TypeRuntimeRegistry,
    ) -> Result<Vec<CommittedColumnEntry>, SetupError> {
        self.cells
            .iter()
            .filter(|(key, _)| key.table == table && key.col == col)
            .map(|(key, value)| {
                type_runtimes
                    .decode_portable(value)
                    .map(|typed| CommittedColumnEntry {
                        key: key.key.clone(),
                        value: typed,
                        is_null: false,
                    })
                    .map_err(|error| SetupError::Validation {
                        detail: format!(
                            "failed to decode committed cell ({}, {}, {}): {error}",
                            key.table.0, key.col.0, key.key
                        ),
                    })
            })
            .collect()
    }

    #[cfg(feature = "prove")]
    pub(crate) fn committed_entries(
        &self,
        table: TableId,
        col: ColId,
        type_runtimes: &TypeRuntimeRegistry,
    ) -> Result<Vec<CommittedEntry>, SetupError> {
        self.committed_column_entries(table, col, type_runtimes)?
            .into_iter()
            .map(|entry| {
                Ok(CommittedEntry {
                    key: entry.key,
                    value: entry.value,
                    is_null: entry.is_null,
                })
            })
            .collect()
    }
}

impl StateView for CommittedStateSnapshot {
    fn read(&self, key: &CommittedCellKey) -> Result<Option<PortableValue>, TabulaError> {
        Ok(self.cells.get(key).cloned())
    }

    fn column_entries(
        &self,
        table: TableId,
        col: ColId,
    ) -> Result<Vec<(CommittedKey, PortableValue)>, TabulaError> {
        Ok(self
            .cells
            .iter()
            .filter(|(key, _)| key.table == table && key.col == col)
            .map(|(key, value)| (key.key.clone(), value.clone()))
            .collect())
    }
}
