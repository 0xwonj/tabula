//! Witness generation: transforms `ExecutionResult` into `BatchWitness`.

use std::collections::{BTreeMap, BTreeSet};

use p3_baby_bear::BabyBear;

use tabula_commitment::{
    BabyBearCodec, ColumnMeta, ColumnState, FieldHasher, HybridVC, NativeDigest,
};
use tabula_core::error::TabulaError;
use tabula_core::traits::ValueCodec;
use tabula_core::{
    ColId, ExecutionResult, OpKind, RowKey, TableId, TableSchema, Value, ValueType, zero_value,
};

use super::route::route_keys;
use super::types::{AccessRow, BatchWitness, ColumnWitness, InitRow};

/// Per-column writes: `(row_key, encoded_value)` pairs. `None` = delete.
type ColumnWrites = Vec<(RowKey, Option<Vec<BabyBear>>)>;

/// Result of `build_column_witnesses`: per-column witnesses, flat column metas,
/// and the post-batch column states.
type ColumnWitnessResult<H> = (
    Vec<ColumnWitness<H>>,
    Vec<ColumnMeta>,
    BTreeMap<(TableId, ColId), ColumnState<H>>,
);

/// Generates structured witness data from execution results.
///
/// Bridges the executor's `ExecutionResult` to proof-system trace tables
/// by encoding values into field elements and computing state transitions.
pub struct WitnessGenerator<H: FieldHasher<F = BabyBear, Digest = NativeDigest>> {
    vc: HybridVC<H>,
    codec: BabyBearCodec,
}

impl<H: FieldHasher<F = BabyBear, Digest = NativeDigest>> WitnessGenerator<H> {
    /// Create a new witness generator.
    pub fn new(vc: HybridVC<H>) -> Self {
        Self {
            vc,
            codec: BabyBearCodec,
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
        result: &ExecutionResult,
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
                return Err(TabulaError::ConsistencyError(format!(
                    "touched column ({:?}, {:?}) not in old_column_states",
                    tc.0, tc.1
                )));
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
        );

        // 7. Compute old and new state roots.
        let old_state_root = self.compute_state_root(old_column_states)?;
        let new_state_root = self.compute_state_root(&new_column_states)?;

        Ok(BatchWitness {
            columns: column_witnesses,
            column_metas,
            old_state_root,
            new_state_root,
            tx_outcomes: result.tx_outcomes.clone(),
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
    ) -> ColumnWitnessResult<H> {
        let mut column_witnesses = Vec::new();
        let mut new_column_states: BTreeMap<(TableId, ColId), ColumnState<H>> = BTreeMap::new();
        let mut column_metas: Vec<ColumnMeta> = Vec::new();

        // Process all columns present in old_column_states (touched + untouched).
        for (&(table, col), old_state) in old_column_states {
            let is_touched = touched.contains(&(table, col));
            let com_old = self.vc.column_commitment(old_state);

            let (new_state, com_new, merge_trace) = if is_touched {
                let writes = writes_by_col
                    .get(&(table, col))
                    .map(|w| w.as_slice())
                    .unwrap_or(&[]);
                self.vc.apply_column_writes(old_state, table, col, writes)
            } else {
                (old_state.clone(), com_old, None)
            };

            // type_map covers all columns in old_column_states (built from all_columns).
            let value_type = type_map[&(table, col)];

            let meta = ColumnMeta {
                table,
                col,
                tag: old_state.strategy(),
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

        (column_witnesses, column_metas, new_column_states)
    }

    /// Collect all `(table, col)` pairs touched by events or writes.
    fn collect_touched(result: &ExecutionResult) -> BTreeSet<(TableId, ColId)> {
        let mut touched = BTreeSet::new();
        for event in &result.events {
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
            let schema = schemas.get(&table).ok_or_else(|| {
                TabulaError::ConsistencyError(format!("no schema for table {:?}", table))
            })?;
            let col_def = schema.columns.iter().find(|c| c.id == col).ok_or_else(|| {
                TabulaError::ConsistencyError(format!("no column {:?} in table {:?}", col, table))
            })?;
            type_map.insert((table, col), col_def.value_type);
        }
        Ok(type_map)
    }

    /// Encode a value as Tier 1 ComEnc field elements, using canonical zero when null.
    ///
    /// Unified null-encoding logic: when `is_null` is true, encodes the canonical
    /// zero for the given `value_type` regardless of the `value` argument.
    fn encode_value_with_null_flag(
        &self,
        value: &Value,
        is_null: bool,
        value_type: ValueType,
    ) -> Result<(Vec<BabyBear>, bool), TabulaError> {
        if is_null {
            let zero = zero_value(value_type);
            let fes = self.codec.encode(&zero)?;
            Ok((fes, true))
        } else {
            let fes = self.codec.encode(value)?;
            Ok((fes, false))
        }
    }

    /// Encode an `Option<Value>` as Tier 1 ComEnc field elements.
    ///
    /// `None` maps to canonical zero (null). Delegates to `encode_value_with_null_flag`.
    fn encode_value(
        &self,
        value: &Option<Value>,
        value_type: ValueType,
    ) -> Result<(Vec<BabyBear>, bool), TabulaError> {
        match value {
            Some(v) => self.encode_value_with_null_flag(v, false, value_type),
            None => {
                // Value content is irrelevant when null — zero_value is used inside.
                let placeholder = zero_value(value_type);
                self.encode_value_with_null_flag(&placeholder, true, value_type)
            }
        }
    }

    /// Build init rows from `read_set_old`, grouped by `(t,c)`, sorted by row key.
    fn build_init_rows(
        &self,
        result: &ExecutionResult,
        type_map: &BTreeMap<(TableId, ColId), ValueType>,
    ) -> Result<BTreeMap<(TableId, ColId), Vec<InitRow>>, TabulaError> {
        let mut grouped: BTreeMap<(TableId, ColId), Vec<InitRow>> = BTreeMap::new();

        for (key, value) in &result.read_set_old {
            let tc = (key.table, key.col);
            let value_type = *type_map.get(&tc).ok_or_else(|| {
                TabulaError::ConsistencyError(format!(
                    "no type for ({:?}, {:?}) in init row",
                    key.table, key.col
                ))
            })?;
            let (fes, is_null) = self.encode_value(value, value_type)?;
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

    /// Build access rows from `events`, grouped by `(t,c)`, preserving event order.
    fn build_access_rows(
        &self,
        result: &ExecutionResult,
        type_map: &BTreeMap<(TableId, ColId), ValueType>,
    ) -> Result<BTreeMap<(TableId, ColId), Vec<AccessRow>>, TabulaError> {
        let mut grouped: BTreeMap<(TableId, ColId), Vec<AccessRow>> = BTreeMap::new();

        for event in &result.events {
            let tc = (event.key.table, event.key.col);
            let value_type = *type_map.get(&tc).ok_or_else(|| {
                TabulaError::ConsistencyError(format!(
                    "no type for ({:?}, {:?}) in access row",
                    event.key.table, event.key.col
                ))
            })?;

            let (fes, is_null) =
                self.encode_value_with_null_flag(&event.value, event.val_is_null, value_type)?;

            grouped.entry(tc).or_default().push(AccessRow {
                key: event.key,
                time: event.time,
                is_write: event.op == OpKind::Write,
                value_fes: fes,
                val_is_null: is_null,
                tx_index: event.tx_index,
                effect_ordinal_in_tx: event.effect_ordinal_in_tx,
            });
        }

        Ok(grouped)
    }

    /// Group writes from `write_set_final` by `(t,c)`, sorted by row key.
    ///
    /// Returns writes as `(RowKey, Option<Vec<BabyBear>>)` — `None` for deletes.
    fn group_writes(
        &self,
        result: &ExecutionResult,
        type_map: &BTreeMap<(TableId, ColId), ValueType>,
    ) -> Result<BTreeMap<(TableId, ColId), ColumnWrites>, TabulaError> {
        let mut grouped: BTreeMap<(TableId, ColId), ColumnWrites> = BTreeMap::new();

        for (key, value) in &result.write_set_final {
            let tc = (key.table, key.col);
            // Validate type exists for this column.
            type_map.get(&tc).ok_or_else(|| {
                TabulaError::ConsistencyError(format!(
                    "no type for ({:?}, {:?}) in write set",
                    key.table, key.col
                ))
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

    /// Compute the two-level state root from column states.
    fn compute_state_root(
        &self,
        column_states: &BTreeMap<(TableId, ColId), ColumnState<H>>,
    ) -> Result<NativeDigest, TabulaError> {
        // Group by table → columns.
        let mut tables: BTreeMap<TableId, BTreeMap<ColId, NativeDigest>> = BTreeMap::new();
        for (&(table, col), state) in column_states {
            let com = self.vc.column_commitment(state);
            let leaf = self.vc.compute_leaf(table, col, state.strategy(), &com);
            tables.entry(table).or_default().insert(col, leaf);
        }

        // table roots → state root.
        let mut table_roots = BTreeMap::new();
        for (table, col_leaves) in &tables {
            table_roots.insert(*table, self.vc.compute_table_root(col_leaves));
        }

        Ok(self.vc.compute_state_root(&table_roots))
    }
}
