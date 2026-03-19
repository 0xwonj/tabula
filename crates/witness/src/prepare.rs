//! Minimal public preparation helpers for runtime-owned proof input assembly.

use std::collections::{BTreeMap, BTreeSet};

use p3_koala_bear::KoalaBear;

use tabula_commitment::{
    ColumnMeta, FieldHasher, KoalaBearCodec, NativeDigest, compute_leaf, compute_table_root,
};
use tabula_core::error::TabulaError;
use tabula_core::traits::ValueCodec;
use tabula_core::{BatchResult, ColId, OpKind, RowKey, TableId, TableSchema, ValueType};

use crate::witness::encoding::{encode_value, encode_value_with_null_flag};
use crate::{AccessRow, InitRow};

/// Per-column writes: `(row_key, encoded_value)` pairs. `None` = delete.
pub type EncodedColumnWrites = Vec<(RowKey, Option<Vec<KoalaBear>>)>;

/// Shared execution-derived inputs used by runtime-owned transition backends.
#[derive(Clone)]
pub struct PreparedExecutionInputs {
    /// Columns touched by reads, writes, or execution events in this batch.
    pub touched: BTreeSet<(TableId, ColId)>,
    /// Schema-derived value types for every planned column.
    pub type_map: BTreeMap<(TableId, ColId), ValueType>,
    /// Base-state init rows grouped by column.
    pub init_rows_by_col: BTreeMap<(TableId, ColId), Vec<InitRow>>,
    /// Access rows grouped by column in execution order.
    pub access_rows_by_col: BTreeMap<(TableId, ColId), Vec<AccessRow>>,
    /// Final coalesced writes grouped by column.
    pub writes_by_col: BTreeMap<(TableId, ColId), EncodedColumnWrites>,
}

/// Minimal public helper for runtime-owned proof-input preparation.
///
/// This surface intentionally exposes only the shared execution-row
/// preparation and state-root helpers used by runtime-owned transition
/// backends.
pub struct ExecutionInputPreparer<H: FieldHasher<F = KoalaBear, Digest = NativeDigest>> {
    hasher: H,
    codec: KoalaBearCodec,
}

impl<H: FieldHasher<F = KoalaBear, Digest = NativeDigest>> ExecutionInputPreparer<H> {
    /// Create a new preparer with the given field hasher.
    pub fn new(hasher: H) -> Self {
        Self {
            hasher,
            codec: KoalaBearCodec,
        }
    }

    /// Prepare shared execution-derived inputs for runtime-owned transition backends.
    pub fn prepare_execution_inputs<'a>(
        &self,
        result: &BatchResult,
        schemas: &BTreeMap<TableId, TableSchema>,
        all_columns: impl IntoIterator<Item = &'a (TableId, ColId)>,
    ) -> Result<PreparedExecutionInputs, TabulaError> {
        let all_columns: BTreeSet<(TableId, ColId)> = all_columns.into_iter().copied().collect();
        let touched = Self::collect_touched(result);

        for tc in &touched {
            if !all_columns.contains(tc) {
                return Err(TabulaError::ProofError {
                    phase: "witness",
                    detail: format!(
                        "touched column ({:?}, {:?}) not in planned columns",
                        tc.0, tc.1
                    ),
                });
            }
        }

        let type_map = Self::build_type_map(schemas, all_columns.iter())?;
        let init_rows_by_col = self.build_init_rows(result, &type_map)?;
        let access_rows_by_col = self.build_access_rows(result, &type_map)?;
        let writes_by_col = self.group_writes(result, &type_map)?;

        Ok(PreparedExecutionInputs {
            touched,
            type_map,
            init_rows_by_col,
            access_rows_by_col,
            writes_by_col,
        })
    }

    /// Compute the old/new state roots from verifier-visible column metadata.
    pub fn compute_state_roots_from_metas(
        &self,
        metas: &[ColumnMeta],
    ) -> (NativeDigest, NativeDigest) {
        let mut old_tables: BTreeMap<TableId, BTreeMap<ColId, NativeDigest>> = BTreeMap::new();
        let mut new_tables: BTreeMap<TableId, BTreeMap<ColId, NativeDigest>> = BTreeMap::new();

        for meta in metas {
            old_tables.entry(meta.table).or_default().insert(
                meta.col,
                compute_leaf(&self.hasher, meta.table, meta.col, meta.tag, &meta.com_old),
            );
            new_tables.entry(meta.table).or_default().insert(
                meta.col,
                compute_leaf(&self.hasher, meta.table, meta.col, meta.tag, &meta.com_new),
            );
        }

        let old_roots: BTreeMap<_, _> = old_tables
            .iter()
            .map(|(table, leaves)| (*table, compute_table_root(&self.hasher, leaves)))
            .collect();
        let new_roots: BTreeMap<_, _> = new_tables
            .iter()
            .map(|(table, leaves)| (*table, compute_table_root(&self.hasher, leaves)))
            .collect();

        (
            tabula_commitment::compute_state_root(&self.hasher, &old_roots),
            tabula_commitment::compute_state_root(&self.hasher, &new_roots),
        )
    }

    /// Collect all `(table, col)` pairs touched by events or writes.
    fn collect_touched(result: &BatchResult) -> BTreeSet<(TableId, ColId)> {
        let mut touched = BTreeSet::new();
        for event in result.successful_events() {
            touched.insert((event.key.table, event.key.col));
        }
        for (key, _) in &result.write_set_final {
            touched.insert((key.table, key.col));
        }
        touched
    }

    /// Build a `(table, col) -> ValueType` mapping from schemas.
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

    /// Build init rows from `read_set_old`, grouped by `(t,c)`, sorted by row key.
    fn build_init_rows(
        &self,
        result: &BatchResult,
        type_map: &BTreeMap<(TableId, ColId), ValueType>,
    ) -> Result<BTreeMap<(TableId, ColId), Vec<InitRow>>, TabulaError> {
        let mut grouped: BTreeMap<(TableId, ColId), Vec<InitRow>> = BTreeMap::new();

        for (key, value) in &result.read_set_old {
            let tc = (key.table, key.col);
            let value_type = *type_map.get(&tc).ok_or_else(|| TabulaError::ProofError {
                phase: "witness",
                detail: format!("no type for ({:?}, {:?}) in init row", key.table, key.col),
            })?;
            let (fes, is_null) = encode_value(&self.codec, value, value_type)?;
            grouped.entry(tc).or_default().push(InitRow {
                key: *key,
                value_fes: fes,
                val_is_null: is_null,
            });
        }

        for rows in grouped.values_mut() {
            rows.sort_by_key(|r| r.key.row);
        }

        Ok(grouped)
    }

    /// Build access rows from successful events, grouped by `(t,c)`, preserving event order.
    fn build_access_rows(
        &self,
        result: &BatchResult,
        type_map: &BTreeMap<(TableId, ColId), ValueType>,
    ) -> Result<BTreeMap<(TableId, ColId), Vec<AccessRow>>, TabulaError> {
        let mut grouped: BTreeMap<(TableId, ColId), Vec<AccessRow>> = BTreeMap::new();

        for (tx_index, event) in result.successful_events_with_tx() {
            let tc = (event.key.table, event.key.col);
            let value_type = *type_map.get(&tc).ok_or_else(|| TabulaError::ProofError {
                phase: "witness",
                detail: format!(
                    "no type for ({:?}, {:?}) in access row",
                    event.key.table, event.key.col
                ),
            })?;

            let (fes, is_null) = encode_value_with_null_flag(
                &self.codec,
                &event.value,
                event.val_is_null,
                value_type,
            )?;

            grouped.entry(tc).or_default().push(AccessRow {
                key: event.key,
                time: event.time,
                is_write: event.op == OpKind::Write,
                value_fes: fes,
                val_is_null: is_null,
                tx_index,
                effect_ordinal_in_tx: event.effect_ordinal_in_tx,
            });
        }

        Ok(grouped)
    }

    /// Group writes from `write_set_final` by `(t,c)`, sorted by row key.
    fn group_writes(
        &self,
        result: &BatchResult,
        type_map: &BTreeMap<(TableId, ColId), ValueType>,
    ) -> Result<BTreeMap<(TableId, ColId), EncodedColumnWrites>, TabulaError> {
        let mut grouped: BTreeMap<(TableId, ColId), EncodedColumnWrites> = BTreeMap::new();

        for (key, value) in &result.write_set_final {
            let tc = (key.table, key.col);
            type_map.get(&tc).ok_or_else(|| TabulaError::ProofError {
                phase: "witness",
                detail: format!("no type for ({:?}, {:?}) in write set", key.table, key.col),
            })?;

            let encoded = match value {
                Some(v) => Some(self.codec.encode(v)?),
                None => None,
            };

            grouped.entry(tc).or_default().push((key.row, encoded));
        }

        for writes in grouped.values_mut() {
            writes.sort_by_key(|(k, _)| *k);
        }

        Ok(grouped)
    }
}
