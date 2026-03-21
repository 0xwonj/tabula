//! Minimal public preparation helpers for runtime-owned proof input assembly.

use std::collections::{BTreeMap, BTreeSet};

use tabula_core::error::TabulaError;
use tabula_core::{BatchResult, ColId, OpKind, TableId, TableSchema, ValueType, zero_value};

use crate::{AccessEvent, ColumnWrite, InitCell};

type LogicalColumnWritesByColumn = BTreeMap<(TableId, ColId), Vec<ColumnWrite>>;

/// Ordered logical proof inputs for one planned committed column.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedExecutionColumn {
    /// Table identifier.
    pub table: TableId,
    /// Column identifier.
    pub col: ColId,
    /// Declared value type for the column.
    pub value_type: ValueType,
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

        let type_map = Self::build_type_map(schemas, all_columns.iter())?;
        let mut init_cells_by_col = Self::build_init_cells(result, &type_map)?;
        let mut access_events_by_col = Self::build_access_events(result, &type_map)?;
        let mut writes_by_col = Self::group_writes(result, &type_map)?;

        let columns = all_columns
            .into_iter()
            .map(|(table, col)| PreparedExecutionColumn {
                table,
                col,
                value_type: type_map[&(table, col)],
                init_cells: init_cells_by_col.remove(&(table, col)).unwrap_or_default(),
                access_events: access_events_by_col
                    .remove(&(table, col))
                    .unwrap_or_default(),
                writes: writes_by_col.remove(&(table, col)).unwrap_or_default(),
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

    fn build_type_map<'a>(
        schemas: &BTreeMap<TableId, TableSchema>,
        all_columns: impl IntoIterator<Item = &'a (TableId, ColId)>,
    ) -> Result<BTreeMap<(TableId, ColId), ValueType>, TabulaError> {
        let mut type_map = BTreeMap::new();
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
            type_map.insert((table, col), col_def.value_type);
        }
        Ok(type_map)
    }

    fn build_init_cells(
        result: &BatchResult,
        type_map: &BTreeMap<(TableId, ColId), ValueType>,
    ) -> Result<BTreeMap<(TableId, ColId), Vec<InitCell>>, TabulaError> {
        let mut grouped: BTreeMap<(TableId, ColId), Vec<InitCell>> = BTreeMap::new();

        for (key, value) in &result.read_set_old {
            let tc = (key.table, key.col);
            let value_type = *type_map.get(&tc).ok_or_else(|| TabulaError::ProofError {
                phase: "witness",
                detail: format!("no type for ({:?}, {:?}) in init cell", key.table, key.col),
            })?;
            grouped.entry(tc).or_default().push(InitCell {
                key: *key,
                value: (*value).unwrap_or_else(|| zero_value(value_type)),
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
        type_map: &BTreeMap<(TableId, ColId), ValueType>,
    ) -> Result<BTreeMap<(TableId, ColId), Vec<AccessEvent>>, TabulaError> {
        let mut grouped: BTreeMap<(TableId, ColId), Vec<AccessEvent>> = BTreeMap::new();

        for (tx_index, event) in result.successful_events_with_tx() {
            let tc = (event.key.table, event.key.col);
            type_map.get(&tc).ok_or_else(|| TabulaError::ProofError {
                phase: "witness",
                detail: format!(
                    "no type for ({:?}, {:?}) in access event",
                    event.key.table, event.key.col
                ),
            })?;
            grouped.entry(tc).or_default().push(AccessEvent {
                key: event.key,
                time: event.time,
                is_write: event.op == OpKind::Write,
                value: event.value,
                is_null: event.val_is_null,
                tx_index,
                effect_ordinal_in_tx: event.effect_ordinal_in_tx,
            });
        }

        Ok(grouped)
    }

    fn group_writes(
        result: &BatchResult,
        type_map: &BTreeMap<(TableId, ColId), ValueType>,
    ) -> Result<LogicalColumnWritesByColumn, TabulaError> {
        let mut grouped: LogicalColumnWritesByColumn = BTreeMap::new();

        for (key, value) in &result.write_set_final {
            let tc = (key.table, key.col);
            type_map.get(&tc).ok_or_else(|| TabulaError::ProofError {
                phase: "witness",
                detail: format!("no type for ({:?}, {:?}) in write set", key.table, key.col),
            })?;
            grouped.entry(tc).or_default().push(ColumnWrite {
                row: key.row,
                value: *value,
            });
        }

        for writes in grouped.values_mut() {
            writes.sort_by_key(|write| write.row);
        }

        Ok(grouped)
    }
}
