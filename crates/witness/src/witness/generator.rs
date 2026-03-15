//! Witness generation: transforms `BatchResult` into `BatchWitness`.

use std::collections::{BTreeMap, BTreeSet};

use p3_koala_bear::KoalaBear;

use tabula_commitment::{ColumnMeta, ColumnState, FieldHasher, KoalaBearCodec, NativeDigest};
use tabula_core::error::TabulaError;
use tabula_core::traits::ValueCodec;
use tabula_core::{BatchResult, ColId, OpKind, RowKey, TableId, TableSchema, ValueType};

use super::encoding::{
    compute_state_root, encode_value, encode_value_with_null_flag, proof_column_commitment,
};
use super::route::route_keys;
use super::types::{AccessRow, BatchWitness, ColumnWitness, InitRow};

/// Per-column writes: `(row_key, encoded_value)` pairs. `None` = delete.
type ColumnWrites = Vec<(RowKey, Option<Vec<KoalaBear>>)>;

/// Result of `build_column_witnesses`: per-column witnesses, flat column metas,
/// and the post-batch column states.
type ColumnWitnessResult<H> = (
    Vec<ColumnWitness<H>>,
    Vec<ColumnMeta>,
    BTreeMap<(TableId, ColId), ColumnState<H>>,
);

/// Generates structured witness data from execution results.
///
/// Bridges the executor's `BatchResult` to proof-system trace tables
/// by encoding values into field elements and computing state transitions.
pub struct WitnessGenerator<H: FieldHasher<F = KoalaBear, Digest = NativeDigest>> {
    hasher: H,
    codec: KoalaBearCodec,
}

impl<H: FieldHasher<F = KoalaBear, Digest = NativeDigest>> WitnessGenerator<H> {
    /// Create a new witness generator with the given field hasher.
    pub fn new(hasher: H) -> Self {
        Self {
            hasher,
            codec: KoalaBearCodec,
        }
    }

    /// Generate a `BatchWitness` from an execution result.
    ///
    /// # Arguments
    /// - `result`: Output of deterministic batch execution.
    /// - `schemas`: Table schemas (needed for value type lookup).
    /// - `old_column_states`: Pre-batch column states for all columns referenced.
    pub fn generate(
        &self,
        result: &BatchResult,
        schemas: &BTreeMap<TableId, TableSchema>,
        old_column_states: &BTreeMap<(TableId, ColId), ColumnState<H>>,
    ) -> Result<BatchWitness<H>, TabulaError> {
        // 1. Collect all touched (t,c) pairs from events + write_set_final.
        let touched = Self::collect_touched(result);

        // 1a. Route keys for downstream proof-path selection.
        let key_routes = route_keys(result);

        // 1b. Validate that all touched columns exist in old_column_states.
        for tc in &touched {
            if !old_column_states.contains_key(tc) {
                return Err(TabulaError::ProofError {
                    phase: "witness",
                    detail: format!(
                        "touched column ({:?}, {:?}) not in old_column_states",
                        tc.0, tc.1
                    ),
                });
            }
        }

        // 2. Build schema lookup: (t, c) → ValueType for ALL columns (not just touched).
        let type_map = Self::build_type_map(schemas, old_column_states.keys())?;

        // 3. Build init rows from read_set_old, grouped by (t,c).
        let mut init_rows_by_col = self.build_init_rows(result, &type_map)?;

        // 4. Build access rows from events, grouped by (t,c).
        let mut access_rows_by_col = self.build_access_rows(result, &type_map)?;

        // 5. Group writes by (t,c), sorted by row key.
        let writes_by_col = self.group_writes(result, &type_map)?;

        // 6. Build per-column witnesses.
        let (column_witnesses, column_metas, new_column_states) = self.build_column_witnesses(
            old_column_states,
            &touched,
            &type_map,
            &writes_by_col,
            &mut init_rows_by_col,
            &mut access_rows_by_col,
        )?;

        // 7. Compute old and new state roots.
        let old_state_root = compute_state_root(&self.hasher, old_column_states)?;
        let new_state_root = compute_state_root(&self.hasher, &new_column_states)?;

        Ok(BatchWitness {
            columns: column_witnesses,
            column_metas,
            old_state_root,
            new_state_root,
            tx_results: result.txs.clone(),
            key_routes,
        })
    }

    /// Build per-column witnesses, column metas, and new column states.
    ///
    /// Iterates all columns in `old_column_states` (touched + untouched),
    /// applying writes for touched columns and producing `ColumnWitness`,
    /// `ColumnMeta`, and the post-batch `ColumnState` for each.
    fn build_column_witnesses(
        &self,
        old_column_states: &BTreeMap<(TableId, ColId), ColumnState<H>>,
        touched: &BTreeSet<(TableId, ColId)>,
        type_map: &BTreeMap<(TableId, ColId), ValueType>,
        writes_by_col: &BTreeMap<(TableId, ColId), ColumnWrites>,
        init_rows_by_col: &mut BTreeMap<(TableId, ColId), Vec<InitRow>>,
        access_rows_by_col: &mut BTreeMap<(TableId, ColId), Vec<AccessRow>>,
    ) -> Result<ColumnWitnessResult<H>, TabulaError> {
        let mut column_witnesses = Vec::new();
        let mut new_column_states: BTreeMap<(TableId, ColId), ColumnState<H>> = BTreeMap::new();
        let mut column_metas: Vec<ColumnMeta> = Vec::new();

        // Process all columns present in old_column_states (touched + untouched).
        for (&(table, col), old_state) in old_column_states {
            let is_touched = touched.contains(&(table, col));
            let com_old = proof_column_commitment(table, col, old_state)?;

            let (new_state, _runtime_com_new, merge_trace) = if is_touched {
                let writes = writes_by_col
                    .get(&(table, col))
                    .map_or(&[][..], Vec::as_slice);
                old_state.apply_writes(&self.hasher, table, col, writes)
            } else {
                (old_state.clone(), com_old, None)
            };
            let com_new = proof_column_commitment(table, col, &new_state)?;

            // type_map covers all columns in old_column_states (built from all_columns).
            let value_type = type_map[&(table, col)];

            let meta = ColumnMeta {
                table,
                col,
                tag: old_state.scheme_tag(),
                com_old,
                com_new,
                is_empty_old: old_state.is_empty(),
                is_empty_new: new_state.is_empty(),
                is_touched,
            };

            let init_rows = init_rows_by_col.remove(&(table, col)).unwrap_or_default();
            let access_rows = access_rows_by_col.remove(&(table, col)).unwrap_or_default();

            column_witnesses.push(ColumnWitness {
                table,
                col,
                value_type,
                init_rows,
                access_rows,
                old_state: old_state.clone(),
                new_state: new_state.clone(),
                merge_trace,
                meta: meta.clone(),
            });

            new_column_states.insert((table, col), new_state);
            column_metas.push(meta);
        }

        Ok((column_witnesses, column_metas, new_column_states))
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

    /// Build a (table, col) → ValueType mapping from schemas.
    ///
    /// Includes all columns in `all_columns` (not just touched), so that
    /// untouched columns also get their correct type.
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

        // Sort each group by row key.
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
    ///
    /// Returns writes as `(RowKey, Option<Vec<KoalaBear>>)` — `None` for deletes.
    fn group_writes(
        &self,
        result: &BatchResult,
        type_map: &BTreeMap<(TableId, ColId), ValueType>,
    ) -> Result<BTreeMap<(TableId, ColId), ColumnWrites>, TabulaError> {
        let mut grouped: BTreeMap<(TableId, ColId), ColumnWrites> = BTreeMap::new();

        for (key, value) in &result.write_set_final {
            let tc = (key.table, key.col);
            // Validate type exists for this column.
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

        // Sort each group by row key.
        for writes in grouped.values_mut() {
            writes.sort_by_key(|(k, _)| *k);
        }

        Ok(grouped)
    }
}
