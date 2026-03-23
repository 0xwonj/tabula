//! Minimal public preparation helpers for runtime-owned proof input assembly.

use std::collections::{BTreeMap, BTreeSet};

use tabula_core::error::TabulaError;
use tabula_core::{BatchResult, ColId, OpKind, TableId, TableSchema};
use tabula_profile::ProfileCatalog;
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry};

use crate::{AccessEvent, ColumnValueProfile, ColumnWrite, InitCell};

type LogicalColumnWritesByColumn = BTreeMap<(TableId, ColId), Vec<ColumnWrite>>;

/// Ordered logical proof inputs for one planned committed column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedExecutionColumn {
    /// Table identifier.
    pub table: TableId,
    /// Column identifier.
    pub col: ColId,
    /// Declared semantic type id for the column.
    pub type_id: tabula_core::TypeId,
    /// Declared encoding profile id for the column.
    pub encoding_profile_id: tabula_core::EncodingProfileId,
    /// Base-state init cells grouped for this column.
    pub init_cells: Vec<InitCell>,
    /// Execution access events for this column.
    pub access_events: Vec<AccessEvent>,
    /// Final coalesced writes for this column.
    pub writes: Vec<ColumnWrite>,
}

impl PreparedExecutionColumn {
    /// Whether the batch contains at least one effective final write.
    pub fn is_touched(&self) -> bool {
        !self.writes.is_empty()
    }
}

/// Shared execution-derived columns used by runtime-owned proof assembly.
#[derive(Clone, Debug)]
pub struct PreparedExecutionColumns {
    /// Ordered per-column logical proof inputs in requested column order.
    pub columns: Vec<PreparedExecutionColumn>,
}

impl PreparedExecutionColumns {
    /// Find one prepared column by `(table, col)`.
    pub fn column(&self, table: TableId, col: ColId) -> Option<&PreparedExecutionColumn> {
        self.columns
            .iter()
            .find(|column| column.table == table && column.col == col)
    }
}

/// Minimal public helper for runtime-owned proof-input preparation.
#[derive(Clone, Copy, Debug, Default)]
pub struct ExecutionInputPreparer;

impl ExecutionInputPreparer {
    /// Create a new preparer.
    pub fn new() -> Self {
        Self
    }

    /// Prepare shared execution-derived logical inputs for runtime-owned proof assembly.
    pub fn prepare_execution_inputs<'a>(
        &self,
        result: &BatchResult,
        schemas: &BTreeMap<TableId, TableSchema>,
        profile_catalog: &ProfileCatalog,
        type_runtimes: &TypeRuntimeRegistry,
        encoding_runtimes: &EncodingRuntimeRegistry,
        all_columns: impl IntoIterator<Item = &'a (TableId, ColId)>,
    ) -> Result<PreparedExecutionColumns, TabulaError> {
        let all_columns: Vec<(TableId, ColId)> = all_columns.into_iter().copied().collect();
        let planned_columns: BTreeSet<(TableId, ColId)> = all_columns.iter().copied().collect();

        if planned_columns.len() != all_columns.len() {
            return Err(TabulaError::ProofError {
                phase: "witness",
                detail: "duplicate planned column in execution-input preparation".to_string(),
            });
        }

        let written_columns = Self::collect_written_columns(result);

        for tc in &written_columns {
            if !planned_columns.contains(tc) {
                return Err(TabulaError::ProofError {
                    phase: "witness",
                    detail: format!(
                        "written column ({:?}, {:?}) not in planned columns",
                        tc.0, tc.1
                    ),
                });
            }
        }

        let profile_map = Self::build_profile_map(
            schemas,
            profile_catalog,
            type_runtimes,
            encoding_runtimes,
            all_columns.iter(),
        )?;
        let mut init_cells_by_col = Self::build_init_cells(result, type_runtimes, &profile_map)?;
        let mut access_events_by_col =
            Self::build_access_events(result, type_runtimes, &profile_map)?;
        let mut writes_by_col = Self::group_writes(result, type_runtimes, &profile_map)?;

        let columns = all_columns
            .into_iter()
            .map(|(table, col)| {
                let profile = profile_map
                    .get(&(table, col))
                    .expect("planned column profile must exist");
                PreparedExecutionColumn {
                    table,
                    col,
                    type_id: profile.type_id,
                    encoding_profile_id: profile.encoding_profile_id,
                    init_cells: init_cells_by_col.remove(&(table, col)).unwrap_or_default(),
                    access_events: access_events_by_col
                        .remove(&(table, col))
                        .unwrap_or_default(),
                    writes: writes_by_col.remove(&(table, col)).unwrap_or_default(),
                }
            })
            .collect();

        Ok(PreparedExecutionColumns { columns })
    }

    fn collect_written_columns(result: &BatchResult) -> BTreeSet<(TableId, ColId)> {
        result
            .write_set_final
            .iter()
            .map(|(key, _)| (key.table, key.col))
            .collect()
    }

    fn build_profile_map<'a>(
        schemas: &BTreeMap<TableId, TableSchema>,
        profile_catalog: &ProfileCatalog,
        type_runtimes: &TypeRuntimeRegistry,
        encoding_runtimes: &EncodingRuntimeRegistry,
        all_columns: impl IntoIterator<Item = &'a (TableId, ColId)>,
    ) -> Result<BTreeMap<(TableId, ColId), ColumnValueProfile>, TabulaError> {
        let mut profile_map = BTreeMap::new();
        for &(table, col) in all_columns {
            let schema = schemas.get(&table).ok_or_else(|| TabulaError::ProofError {
                phase: "witness",
                detail: format!("no schema for table {table:?}"),
            })?;
            let col_def = schema.columns.iter().find(|c| c.id == col).ok_or_else(|| {
                TabulaError::ProofError {
                    phase: "witness",
                    detail: format!("no column {col:?} in table {table:?}"),
                }
            })?;
            let resolved = profile_catalog
                .resolve_column_profile(col_def.column_profile_id)
                .map_err(|err| TabulaError::ProofError {
                    phase: "witness",
                    detail: format!(
                        "column profile {} for table {:?} col {:?} is invalid: {err}",
                        col_def.column_profile_id.0, table, col
                    ),
                })?;
            type_runtimes
                .resolve(resolved.type_descriptor.type_id)
                .map_err(|err| TabulaError::ProofError {
                    phase: "witness",
                    detail: format!(
                        "column profile {} references missing type runtime {}: {err}",
                        col_def.column_profile_id.0, resolved.type_descriptor.type_id.0
                    ),
                })?;
            encoding_runtimes
                .resolve(resolved.encoding_profile.encoding_profile_id)
                .map_err(|err| TabulaError::ProofError {
                    phase: "witness",
                    detail: format!(
                        "column profile {} references missing encoding runtime {}: {err}",
                        col_def.column_profile_id.0,
                        resolved.encoding_profile.encoding_profile_id.0
                    ),
                })?;
            profile_map.insert(
                (table, col),
                ColumnValueProfile {
                    type_id: resolved.type_descriptor.type_id,
                    encoding_profile_id: resolved.encoding_profile.encoding_profile_id,
                },
            );
        }
        Ok(profile_map)
    }

    fn build_init_cells(
        result: &BatchResult,
        type_runtimes: &TypeRuntimeRegistry,
        profile_map: &BTreeMap<(TableId, ColId), ColumnValueProfile>,
    ) -> Result<BTreeMap<(TableId, ColId), Vec<InitCell>>, TabulaError> {
        let mut grouped: BTreeMap<(TableId, ColId), Vec<InitCell>> = BTreeMap::new();

        for (key, value) in &result.read_set_old {
            let tc = (key.table, key.col);
            let profile = profile_map
                .get(&tc)
                .ok_or_else(|| TabulaError::ProofError {
                    phase: "witness",
                    detail: format!(
                        "no sealed type/encoding profile for ({:?}, {:?}) in init cell",
                        key.table, key.col
                    ),
                })?;
            let decoded = match value {
                Some(value) => Self::decode_column_value(type_runtimes, profile, value)?,
                None => type_runtimes.zero_of(profile.type_id)?,
            };
            grouped.entry(tc).or_default().push(InitCell {
                key: *key,
                value: decoded,
                is_null: value.is_none(),
            });
        }

        for rows in grouped.values_mut() {
            rows.sort_by_key(|r| r.key.row);
        }

        Ok(grouped)
    }

    fn build_access_events(
        result: &BatchResult,
        type_runtimes: &TypeRuntimeRegistry,
        profile_map: &BTreeMap<(TableId, ColId), ColumnValueProfile>,
    ) -> Result<BTreeMap<(TableId, ColId), Vec<AccessEvent>>, TabulaError> {
        let mut grouped: BTreeMap<(TableId, ColId), Vec<AccessEvent>> = BTreeMap::new();

        for (tx_index, event) in result.successful_events_with_tx() {
            let tc = (event.key.table, event.key.col);
            let profile = profile_map
                .get(&tc)
                .ok_or_else(|| TabulaError::ProofError {
                    phase: "witness",
                    detail: format!(
                        "no sealed type/encoding profile for ({:?}, {:?}) in access event",
                        event.key.table, event.key.col
                    ),
                })?;
            let value = if event.val_is_null {
                type_runtimes.zero_of(profile.type_id)?
            } else {
                Self::decode_column_value(type_runtimes, profile, &event.value)?
            };
            grouped.entry(tc).or_default().push(AccessEvent {
                key: event.key,
                time: event.time,
                is_write: event.op == OpKind::Write,
                value,
                is_null: event.val_is_null,
                tx_index,
                effect_ordinal_in_tx: event.effect_ordinal_in_tx,
            });
        }

        Ok(grouped)
    }

    fn group_writes(
        result: &BatchResult,
        type_runtimes: &TypeRuntimeRegistry,
        profile_map: &BTreeMap<(TableId, ColId), ColumnValueProfile>,
    ) -> Result<LogicalColumnWritesByColumn, TabulaError> {
        let mut grouped: LogicalColumnWritesByColumn = BTreeMap::new();

        for (key, value) in &result.write_set_final {
            let tc = (key.table, key.col);
            let profile = profile_map
                .get(&tc)
                .ok_or_else(|| TabulaError::ProofError {
                    phase: "witness",
                    detail: format!(
                        "no sealed type/encoding profile for ({:?}, {:?}) in write set",
                        key.table, key.col
                    ),
                })?;
            let decoded = value
                .as_ref()
                .map(|value| Self::decode_column_value(type_runtimes, profile, value))
                .transpose()?;
            grouped.entry(tc).or_default().push(ColumnWrite {
                row: key.row,
                value: decoded,
            });
        }

        for writes in grouped.values_mut() {
            writes.sort_by_key(|write| write.row);
        }

        Ok(grouped)
    }

    fn decode_column_value(
        type_runtimes: &TypeRuntimeRegistry,
        profile: &ColumnValueProfile,
        value: &tabula_core::PortableValue,
    ) -> Result<tabula_types::TypedValue, TabulaError> {
        if value.type_id() != profile.type_id {
            return Err(TabulaError::ProofError {
                phase: "witness",
                detail: format!(
                    "portable value type {} does not match sealed column type {}",
                    value.type_id().0,
                    profile.type_id.0
                ),
            });
        }
        type_runtimes.decode_portable(value)
    }
}
