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

#[cfg(test)]
mod tests {
    use super::*;
    use tabula_commitment::MockFieldHasher;
    use tabula_core::{CellKey, ColumnDef, ExecutionEvent, TxOutcome};

    fn mock_vc() -> HybridVC<MockFieldHasher> {
        HybridVC::new(MockFieldHasher, 100)
    }

    fn make_wg() -> WitnessGenerator<MockFieldHasher> {
        WitnessGenerator::new(mock_vc())
    }

    fn t(n: u32) -> TableId {
        TableId(n)
    }
    fn c(n: u16) -> ColId {
        ColId(n)
    }
    fn r(n: u64) -> RowKey {
        RowKey(n)
    }
    fn ck(table: u32, col: u16, row: u64) -> CellKey {
        CellKey {
            table: t(table),
            col: c(col),
            row: r(row),
        }
    }

    fn u64_schema(table: u32, cols: &[u16]) -> TableSchema {
        TableSchema {
            id: t(table),
            name: format!("table_{table}"),
            columns: cols
                .iter()
                .map(|&col| ColumnDef {
                    id: c(col),
                    name: format!("col_{col}"),
                    value_type: ValueType::U64,
                })
                .collect(),
        }
    }

    fn schemas(list: Vec<TableSchema>) -> BTreeMap<TableId, TableSchema> {
        list.into_iter().map(|s| (s.id, s)).collect()
    }

    fn empty_column_state(
        vc: &HybridVC<MockFieldHasher>,
        table: u32,
        col: u16,
    ) -> ((TableId, ColId), ColumnState<MockFieldHasher>) {
        let (state, _) = vc.commit_column(t(table), c(col), vec![]);
        ((t(table), c(col)), state)
    }

    fn column_state_with(
        vc: &HybridVC<MockFieldHasher>,
        table: u32,
        col: u16,
        entries: &[(u64, u64)],
    ) -> ((TableId, ColId), ColumnState<MockFieldHasher>) {
        let codec = BabyBearCodec;
        let enc: Vec<(RowKey, Vec<BabyBear>)> = entries
            .iter()
            .map(|&(k, v)| (r(k), codec.encode(&Value::U64(v)).unwrap()))
            .collect();
        let (state, _) = vc.commit_column(t(table), c(col), enc);
        ((t(table), c(col)), state)
    }

    fn read_event(table: u32, col: u16, row: u64, val: u64, time: u64, tx: u32) -> ExecutionEvent {
        ExecutionEvent {
            key: ck(table, col, row),
            op: OpKind::Read,
            value: Value::U64(val),
            val_is_null: false,
            time,
            tx_index: tx,
        }
    }

    fn write_event(table: u32, col: u16, row: u64, val: u64, time: u64, tx: u32) -> ExecutionEvent {
        ExecutionEvent {
            key: ck(table, col, row),
            op: OpKind::Write,
            value: Value::U64(val),
            val_is_null: false,
            time,
            tx_index: tx,
        }
    }

    fn null_read_event(table: u32, col: u16, row: u64, time: u64, tx: u32) -> ExecutionEvent {
        ExecutionEvent {
            key: ck(table, col, row),
            op: OpKind::Read,
            value: Value::U64(0),
            val_is_null: true,
            time,
            tx_index: tx,
        }
    }

    // ── Init row tests ──────────────────────────────────────────────────

    #[test]
    fn init_rows_from_read_set_present() {
        let wg = make_wg();
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 10), Some(Value::U64(42)))],
            write_set_final: vec![],
            events: vec![read_event(1, 0, 10, 42, 1, 0)],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(10, 42)])].into();
        let witness = wg.generate(&result, &schema, &states).unwrap();

        assert_eq!(witness.columns.len(), 1);
        let col_w = &witness.columns[0];
        assert_eq!(col_w.init_rows.len(), 1);
        assert!(!col_w.init_rows[0].val_is_null);
        assert_eq!(col_w.init_rows[0].key.row, r(10));
    }

    #[test]
    fn init_rows_from_read_set_null() {
        let wg = make_wg();
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 5), None)],
            write_set_final: vec![(ck(1, 0, 5), Some(Value::U64(99)))],
            events: vec![
                null_read_event(1, 0, 5, 1, 0),
                write_event(1, 0, 5, 99, 2, 0),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [empty_column_state(&vc, 1, 0)].into();
        let witness = wg.generate(&result, &schema, &states).unwrap();

        let col_w = &witness.columns[0];
        assert_eq!(col_w.init_rows.len(), 1);
        assert!(col_w.init_rows[0].val_is_null);
        // Canonical zero: encoded U64(0)
        let codec = BabyBearCodec;
        let expected_fes = codec.encode(&Value::U64(0)).unwrap();
        assert_eq!(col_w.init_rows[0].value_fes, expected_fes);
    }

    #[test]
    fn init_rows_sorted_by_key() {
        let wg = make_wg();
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![
                (ck(1, 0, 30), Some(Value::U64(3))),
                (ck(1, 0, 10), Some(Value::U64(1))),
                (ck(1, 0, 20), Some(Value::U64(2))),
            ],
            write_set_final: vec![],
            events: vec![
                read_event(1, 0, 30, 3, 1, 0),
                read_event(1, 0, 10, 1, 2, 0),
                read_event(1, 0, 20, 2, 3, 0),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> =
            [column_state_with(&vc, 1, 0, &[(10, 1), (20, 2), (30, 3)])].into();
        let witness = wg.generate(&result, &schema, &states).unwrap();

        let rows = &witness.columns[0].init_rows;
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0].key.row.0, 10);
        assert_eq!(rows[1].key.row.0, 20);
        assert_eq!(rows[2].key.row.0, 30);
    }

    #[test]
    fn init_rows_multi_column() {
        let wg = make_wg();
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![
                (ck(1, 0, 1), Some(Value::U64(10))),
                (ck(1, 1, 1), Some(Value::U64(20))),
            ],
            write_set_final: vec![],
            events: vec![read_event(1, 0, 1, 10, 1, 0), read_event(1, 1, 1, 20, 2, 0)],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0, 1])]);
        let states: BTreeMap<_, _> = [
            column_state_with(&vc, 1, 0, &[(1, 10)]),
            column_state_with(&vc, 1, 1, &[(1, 20)]),
        ]
        .into();
        let witness = wg.generate(&result, &schema, &states).unwrap();
        assert_eq!(witness.columns.len(), 2);
    }

    // ── Access row tests ────────────────────────────────────────────────

    #[test]
    fn access_rows_read_write() {
        let wg = make_wg();
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
            write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(20)))],
            events: vec![
                read_event(1, 0, 1, 10, 1, 0),
                write_event(1, 0, 1, 20, 2, 0),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
        let witness = wg.generate(&result, &schema, &states).unwrap();

        let access = &witness.columns[0].access_rows;
        assert_eq!(access.len(), 2);
        assert!(!access[0].is_write);
        assert!(access[1].is_write);
    }

    #[test]
    fn access_rows_null_read() {
        let wg = make_wg();
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 5), None)],
            write_set_final: vec![],
            events: vec![null_read_event(1, 0, 5, 1, 0)],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [empty_column_state(&vc, 1, 0)].into();
        let witness = wg.generate(&result, &schema, &states).unwrap();

        let access = &witness.columns[0].access_rows;
        assert_eq!(access.len(), 1);
        assert!(access[0].val_is_null);
    }

    #[test]
    fn access_rows_time_carried_through() {
        let wg = make_wg();
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
            write_set_final: vec![],
            events: vec![read_event(1, 0, 1, 10, 42, 0)],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
        let witness = wg.generate(&result, &schema, &states).unwrap();

        assert_eq!(witness.columns[0].access_rows[0].time, 42);
    }

    #[test]
    fn access_rows_multi_tx() {
        let wg = make_wg();
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
            write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(30)))],
            events: vec![
                read_event(1, 0, 1, 10, 1, 0),
                write_event(1, 0, 1, 20, 2, 0),
                read_event(1, 0, 1, 20, 3, 1),
                write_event(1, 0, 1, 30, 4, 1),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success, TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
        let witness = wg.generate(&result, &schema, &states).unwrap();

        let access = &witness.columns[0].access_rows;
        assert_eq!(access.len(), 4);
        assert_eq!(access[0].tx_index, 0);
        assert_eq!(access[3].tx_index, 1);
    }

    // ── Column witness tests ────────────────────────────────────────────

    #[test]
    fn column_witness_single_write() {
        let wg = make_wg();
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
            write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(20)))],
            events: vec![
                read_event(1, 0, 1, 10, 1, 0),
                write_event(1, 0, 1, 20, 2, 0),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
        let witness = wg.generate(&result, &schema, &states).unwrap();

        let col_w = &witness.columns[0];
        assert!(col_w.meta.is_touched);
        assert_ne!(col_w.meta.com_old, col_w.meta.com_new);
    }

    #[test]
    fn column_witness_delete() {
        let wg = make_wg();
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
            write_set_final: vec![(ck(1, 0, 1), None)],
            events: vec![
                read_event(1, 0, 1, 10, 1, 0),
                ExecutionEvent {
                    key: ck(1, 0, 1),
                    op: OpKind::Write,
                    value: Value::U64(0),
                    val_is_null: true,
                    time: 2,
                    tx_index: 0,
                },
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
        let witness = wg.generate(&result, &schema, &states).unwrap();

        let col_w = &witness.columns[0];
        assert!(col_w.meta.is_touched);
        assert!(!col_w.meta.is_empty_old);
        assert!(col_w.meta.is_empty_new);
    }

    #[test]
    fn column_witness_untouched() {
        let wg = make_wg();
        let vc = mock_vc();
        // Access col 0, but col 1 is untouched
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
            write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(20)))],
            events: vec![
                read_event(1, 0, 1, 10, 1, 0),
                write_event(1, 0, 1, 20, 2, 0),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0, 1])]);
        let states: BTreeMap<_, _> = [
            column_state_with(&vc, 1, 0, &[(1, 10)]),
            column_state_with(&vc, 1, 1, &[(1, 99)]),
        ]
        .into();
        let witness = wg.generate(&result, &schema, &states).unwrap();

        assert_eq!(witness.columns.len(), 2);
        let untouched = witness
            .columns
            .iter()
            .find(|cw| cw.col == ColId(1))
            .unwrap();
        assert!(!untouched.meta.is_touched);
        assert_eq!(untouched.meta.com_old, untouched.meta.com_new);
    }

    // ── ColumnMeta tests ────────────────────────────────────────────────

    #[test]
    fn column_meta_empty_to_nonempty() {
        let wg = make_wg();
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 1), None)],
            write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(42)))],
            events: vec![
                null_read_event(1, 0, 1, 1, 0),
                write_event(1, 0, 1, 42, 2, 0),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [empty_column_state(&vc, 1, 0)].into();
        let witness = wg.generate(&result, &schema, &states).unwrap();

        let meta = &witness.columns[0].meta;
        assert!(meta.is_empty_old);
        assert!(!meta.is_empty_new);
        assert!(meta.is_touched);
    }

    // ── State root tests ────────────────────────────────────────────────

    #[test]
    fn state_root_deterministic() {
        let wg = make_wg();
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
            write_set_final: vec![],
            events: vec![read_event(1, 0, 1, 10, 1, 0)],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
        let w1 = wg.generate(&result, &schema, &states).unwrap();
        let w2 = wg.generate(&result, &schema, &states).unwrap();
        assert_eq!(w1.old_state_root, w2.old_state_root);
        assert_eq!(w1.new_state_root, w2.new_state_root);
    }

    #[test]
    fn state_root_changes_on_write() {
        let wg = make_wg();
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
            write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(20)))],
            events: vec![
                read_event(1, 0, 1, 10, 1, 0),
                write_event(1, 0, 1, 20, 2, 0),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
        let witness = wg.generate(&result, &schema, &states).unwrap();
        assert_ne!(witness.old_state_root, witness.new_state_root);
    }

    #[test]
    fn state_root_empty_state() {
        let wg = make_wg();
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 1), None)],
            write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(1)))],
            events: vec![
                null_read_event(1, 0, 1, 1, 0),
                write_event(1, 0, 1, 1, 2, 0),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [empty_column_state(&vc, 1, 0)].into();
        let witness = wg.generate(&result, &schema, &states).unwrap();
        assert_ne!(witness.old_state_root, witness.new_state_root);
    }

    // ── End-to-end tests ────────────────────────────────────────────────

    #[test]
    fn e2e_full_flow_single_column() {
        let wg = make_wg();
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![
                (ck(1, 0, 1), Some(Value::U64(10))),
                (ck(1, 0, 2), Some(Value::U64(20))),
            ],
            write_set_final: vec![
                (ck(1, 0, 1), Some(Value::U64(15))),
                (ck(1, 0, 3), Some(Value::U64(30))),
            ],
            events: vec![
                read_event(1, 0, 1, 10, 1, 0),
                read_event(1, 0, 2, 20, 2, 0),
                write_event(1, 0, 1, 15, 3, 0),
                write_event(1, 0, 3, 30, 4, 0),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10), (2, 20)])].into();
        let witness = wg.generate(&result, &schema, &states).unwrap();

        assert_eq!(witness.columns.len(), 1);
        let col_w = &witness.columns[0];
        assert_eq!(col_w.init_rows.len(), 2);
        assert_eq!(col_w.access_rows.len(), 4);
        assert!(col_w.meta.is_touched);
        assert!(col_w.merge_trace.is_some());
        assert_eq!(witness.tx_outcomes.len(), 1);
    }

    #[test]
    fn e2e_two_columns_multi_tx() {
        let wg = make_wg();
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![
                (ck(1, 0, 1), Some(Value::U64(10))),
                (ck(1, 1, 1), Some(Value::U64(100))),
            ],
            write_set_final: vec![
                (ck(1, 0, 1), Some(Value::U64(15))),
                (ck(1, 1, 1), Some(Value::U64(200))),
            ],
            events: vec![
                // tx 0: read+write col 0
                read_event(1, 0, 1, 10, 1, 0),
                write_event(1, 0, 1, 15, 2, 0),
                // tx 1: read+write col 1
                read_event(1, 1, 1, 100, 3, 1),
                write_event(1, 1, 1, 200, 4, 1),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success, TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0, 1])]);
        let states: BTreeMap<_, _> = [
            column_state_with(&vc, 1, 0, &[(1, 10)]),
            column_state_with(&vc, 1, 1, &[(1, 100)]),
        ]
        .into();
        let witness = wg.generate(&result, &schema, &states).unwrap();

        assert_eq!(witness.columns.len(), 2);
        assert!(witness.columns.iter().all(|c| c.meta.is_touched));
        assert_eq!(witness.tx_outcomes.len(), 2);
        assert_ne!(witness.old_state_root, witness.new_state_root);
    }

    #[test]
    fn missing_schema_returns_error() {
        let wg = make_wg();
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![],
            write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
            events: vec![write_event(1, 0, 1, 10, 1, 0)],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![]); // no schemas!
        let states: BTreeMap<_, _> = [empty_column_state(&vc, 1, 0)].into();
        assert!(wg.generate(&result, &schema, &states).is_err());
    }

    #[test]
    fn touched_column_missing_from_old_states_returns_error() {
        let wg = make_wg();
        let result = ExecutionResult {
            read_set_old: vec![],
            write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
            events: vec![write_event(1, 0, 1, 10, 1, 0)],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = BTreeMap::new(); // no column states!
        let err = wg.generate(&result, &schema, &states).unwrap_err();
        assert!(err.to_string().contains("not in old_column_states"));
    }

    #[test]
    fn column_metas_populated_and_sorted() {
        let wg = make_wg();
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![
                (ck(1, 0, 1), Some(Value::U64(10))),
                (ck(1, 1, 1), Some(Value::U64(20))),
            ],
            write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(15)))],
            events: vec![
                read_event(1, 0, 1, 10, 1, 0),
                read_event(1, 1, 1, 20, 2, 0),
                write_event(1, 0, 1, 15, 3, 0),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0, 1])]);
        let states: BTreeMap<_, _> = [
            column_state_with(&vc, 1, 0, &[(1, 10)]),
            column_state_with(&vc, 1, 1, &[(1, 20)]),
        ]
        .into();
        let witness = wg.generate(&result, &schema, &states).unwrap();

        // column_metas should have 2 entries, sorted by (table, col).
        assert_eq!(witness.column_metas.len(), 2);
        assert_eq!(witness.column_metas[0].table, t(1));
        assert_eq!(witness.column_metas[0].col, c(0));
        assert_eq!(witness.column_metas[1].table, t(1));
        assert_eq!(witness.column_metas[1].col, c(1));
        // col 0 was written to, col 1 was only read.
        assert!(witness.column_metas[0].is_touched);
        assert!(witness.column_metas[1].is_touched);
    }

    #[test]
    fn tx_outcomes_preserved() {
        let wg = make_wg();
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
            write_set_final: vec![],
            events: vec![read_event(1, 0, 1, 10, 1, 0)],
            emitted: vec![],
            tx_outcomes: vec![
                TxOutcome::Success,
                TxOutcome::Failed {
                    reason: "overflow".into(),
                    partial_events: vec![],
                    failed_instruction: Some(3),
                },
            ],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
        let witness = wg.generate(&result, &schema, &states).unwrap();
        assert_eq!(witness.tx_outcomes.len(), 2);
        assert!(matches!(witness.tx_outcomes[1], TxOutcome::Failed { .. }));
    }

    // ── M5↔M6 integration: WitnessGenerator → ColumnMeta AIR ───────────

    use crate::air::chips::column_meta::{ColumnMetaChip, generate_column_meta_trace};
    use crate::air::debug::debug_check;

    /// Helper: generate witness, build ColumnMeta trace, verify AIR constraints.
    fn assert_column_meta_air_valid(
        result: &ExecutionResult,
        schema: &BTreeMap<TableId, TableSchema>,
        states: &BTreeMap<(TableId, ColId), ColumnState<MockFieldHasher>>,
    ) {
        let wg = make_wg();
        let witness = wg.generate(result, schema, states).unwrap();
        let trace = generate_column_meta_trace(&witness.column_metas);
        debug_check(&ColumnMetaChip, &trace)
            .expect("ColumnMeta AIR constraints should pass for witness-generated trace");
    }

    #[test]
    fn m5_m6_single_column_write() {
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
            write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(20)))],
            events: vec![
                read_event(1, 0, 1, 10, 1, 0),
                write_event(1, 0, 1, 20, 2, 0),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
        assert_column_meta_air_valid(&result, &schema, &states);
    }

    #[test]
    fn m5_m6_multi_column_touched_and_untouched() {
        let vc = mock_vc();
        // Write to col 0, leave col 1 untouched.
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
            write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(20)))],
            events: vec![
                read_event(1, 0, 1, 10, 1, 0),
                write_event(1, 0, 1, 20, 2, 0),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0, 1])]);
        let states: BTreeMap<_, _> = [
            column_state_with(&vc, 1, 0, &[(1, 10)]),
            column_state_with(&vc, 1, 1, &[(1, 99)]),
        ]
        .into();
        assert_column_meta_air_valid(&result, &schema, &states);
    }

    #[test]
    fn m5_m6_empty_to_nonempty() {
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 1), None)],
            write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(42)))],
            events: vec![
                null_read_event(1, 0, 1, 1, 0),
                write_event(1, 0, 1, 42, 2, 0),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [empty_column_state(&vc, 1, 0)].into();
        assert_column_meta_air_valid(&result, &schema, &states);
    }

    #[test]
    fn m5_m6_delete() {
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
            write_set_final: vec![(ck(1, 0, 1), None)],
            events: vec![
                read_event(1, 0, 1, 10, 1, 0),
                ExecutionEvent {
                    key: ck(1, 0, 1),
                    op: OpKind::Write,
                    value: Value::U64(0),
                    val_is_null: true,
                    time: 2,
                    tx_index: 0,
                },
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
        assert_column_meta_air_valid(&result, &schema, &states);
    }

    #[test]
    fn m5_m6_multi_table_multi_tx() {
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![
                (ck(1, 0, 1), Some(Value::U64(10))),
                (ck(2, 0, 1), Some(Value::U64(100))),
            ],
            write_set_final: vec![
                (ck(1, 0, 1), Some(Value::U64(15))),
                (ck(2, 0, 1), Some(Value::U64(200))),
            ],
            events: vec![
                // tx 0: read+write table 1
                read_event(1, 0, 1, 10, 1, 0),
                write_event(1, 0, 1, 15, 2, 0),
                // tx 1: read+write table 2
                read_event(2, 0, 1, 100, 3, 1),
                write_event(2, 0, 1, 200, 4, 1),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success, TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0]), u64_schema(2, &[0])]);
        let states: BTreeMap<_, _> = [
            column_state_with(&vc, 1, 0, &[(1, 10)]),
            column_state_with(&vc, 2, 0, &[(1, 100)]),
        ]
        .into();
        assert_column_meta_air_valid(&result, &schema, &states);
    }

    #[test]
    fn m5_m6_read_only_no_writes() {
        let vc = mock_vc();
        // Read-only batch: column is touched (via event) but no writes.
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
            write_set_final: vec![],
            events: vec![read_event(1, 0, 1, 10, 1, 0)],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
        assert_column_meta_air_valid(&result, &schema, &states);
    }

    // ── Key routing integration ────────────────────────────────────────

    use crate::witness::route::KeyRoute;

    #[test]
    fn key_routes_populated() {
        let wg = make_wg();
        let vc = mock_vc();
        let k_read = ck(1, 0, 1);
        let k_write = ck(1, 0, 2);
        let result = ExecutionResult {
            read_set_old: vec![
                (k_read, Some(Value::U64(10))),
                (k_write, Some(Value::U64(20))),
            ],
            write_set_final: vec![(k_write, Some(Value::U64(99)))],
            events: vec![
                read_event(1, 0, 1, 10, 1, 0),
                read_event(1, 0, 2, 20, 2, 0),
                write_event(1, 0, 2, 99, 3, 0),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10), (2, 20)])].into();
        let witness = wg.generate(&result, &schema, &states).unwrap();

        assert_eq!(witness.key_routes.len(), 2);
        assert_eq!(witness.key_routes[&k_read], KeyRoute::ReadOnly);
        assert_eq!(witness.key_routes[&k_write], KeyRoute::SortedMemory);
    }

    // ── M5↔M7 integration: WitnessGenerator → GlobalSortedMem AIR ─────

    use crate::air::chips::sorted_mem::air::GlobalSortedMemChip;
    use crate::air::chips::sorted_mem::trace::{SortedMemRow, generate_sorted_mem_trace};

    /// Flatten a BatchWitness into sorted SortedMemRow entries for the
    /// GlobalSortedMem AIR chip. Merges init rows (τ=0) + access rows,
    /// sorted by `(table_id, col_id, row_key, timestamp)`.
    fn flatten_to_sorted_mem_rows(witness: &BatchWitness<MockFieldHasher>) -> Vec<SortedMemRow> {
        let mut rows = Vec::new();

        for cw in &witness.columns {
            for init in &cw.init_rows {
                rows.push(SortedMemRow {
                    table_id: init.key.table.0,
                    col_id: init.key.col.0,
                    row_key: init.key.row.0,
                    timestamp: 0,
                    is_init: true,
                    is_write: false,
                    val: init.value_fes.clone(),
                    val_is_null: init.val_is_null,
                });
            }
            for access in &cw.access_rows {
                rows.push(SortedMemRow {
                    table_id: access.key.table.0,
                    col_id: access.key.col.0,
                    row_key: access.key.row.0,
                    timestamp: access.time,
                    is_init: false,
                    is_write: access.is_write,
                    val: access.value_fes.clone(),
                    val_is_null: access.val_is_null,
                });
            }
        }

        rows.sort_by_key(|r| (r.table_id, r.col_id, r.row_key, r.timestamp));
        rows
    }

    /// Helper: generate witness → flatten → build sorted mem trace → debug AIR check.
    fn assert_sorted_mem_air_valid(
        result: &ExecutionResult,
        schema: &BTreeMap<TableId, TableSchema>,
        states: &BTreeMap<(TableId, ColId), ColumnState<MockFieldHasher>>,
    ) {
        let wg = make_wg();
        let witness = wg.generate(result, schema, states).unwrap();
        let rows = flatten_to_sorted_mem_rows(&witness);
        let trace = generate_sorted_mem_trace::<3>(&rows);
        debug_check(&GlobalSortedMemChip::<3>, &trace)
            .expect("GlobalSortedMem AIR constraints should pass for witness-generated trace");
    }

    #[test]
    fn m5_m7_single_read_write() {
        let vc = mock_vc();
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
            write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(20)))],
            events: vec![
                read_event(1, 0, 1, 10, 1, 0),
                write_event(1, 0, 1, 20, 2, 0),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
        assert_sorted_mem_air_valid(&result, &schema, &states);
    }

    #[test]
    fn m5_m7_multi_key() {
        let vc = mock_vc();
        // Key 1: read+write. Key 2: read-only.
        let result = ExecutionResult {
            read_set_old: vec![
                (ck(1, 0, 1), Some(Value::U64(10))),
                (ck(1, 0, 2), Some(Value::U64(20))),
            ],
            write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(15)))],
            events: vec![
                read_event(1, 0, 1, 10, 1, 0),
                read_event(1, 0, 2, 20, 2, 0),
                write_event(1, 0, 1, 15, 3, 0),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10), (2, 20)])].into();
        assert_sorted_mem_air_valid(&result, &schema, &states);
    }

    #[test]
    fn m5_m7_null_init_write() {
        let vc = mock_vc();
        // Empty cell → write value.
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 1), None)],
            write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(42)))],
            events: vec![
                null_read_event(1, 0, 1, 1, 0),
                write_event(1, 0, 1, 42, 2, 0),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [empty_column_state(&vc, 1, 0)].into();
        assert_sorted_mem_air_valid(&result, &schema, &states);
    }

    #[test]
    fn m5_m7_multi_tx() {
        let vc = mock_vc();
        // Same key accessed across 2 txs.
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
            write_set_final: vec![(ck(1, 0, 1), Some(Value::U64(30)))],
            events: vec![
                read_event(1, 0, 1, 10, 1, 0),
                write_event(1, 0, 1, 20, 2, 0),
                read_event(1, 0, 1, 20, 3, 1),
                write_event(1, 0, 1, 30, 4, 1),
            ],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success, TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
        assert_sorted_mem_air_valid(&result, &schema, &states);
    }

    #[test]
    fn m5_m7_read_only() {
        let vc = mock_vc();
        // Read-only key: init + read, no write.
        let result = ExecutionResult {
            read_set_old: vec![(ck(1, 0, 1), Some(Value::U64(10)))],
            write_set_final: vec![],
            events: vec![read_event(1, 0, 1, 10, 1, 0)],
            emitted: vec![],
            tx_outcomes: vec![TxOutcome::Success],
        };
        let schema = schemas(vec![u64_schema(1, &[0])]);
        let states: BTreeMap<_, _> = [column_state_with(&vc, 1, 0, &[(1, 10)])].into();
        assert_sorted_mem_air_valid(&result, &schema, &states);
    }
}
