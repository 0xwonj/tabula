use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;

use tabula_commitment::{BabyBearCodec, ColumnState, HybridVC, MockFieldHasher};
use tabula_core::traits::ValueCodec;
use tabula_core::{
    CellKey, ColId, ColumnDef, ExecutionEvent, OpKind, RowKey, TableId, TableSchema, Value,
    ValueType,
};

use tabula_witness::WitnessGenerator;

mod generator;
mod program_info;
mod route;

pub(super) fn mock_vc() -> HybridVC<MockFieldHasher> {
    HybridVC::new(MockFieldHasher, 100)
}

pub(super) fn make_wg() -> WitnessGenerator<MockFieldHasher> {
    WitnessGenerator::new(mock_vc())
}

pub(super) fn t(n: u32) -> TableId {
    TableId(n)
}

pub(super) fn c(n: u16) -> ColId {
    ColId(n)
}

pub(super) fn r(n: u64) -> RowKey {
    RowKey(n)
}

pub(super) fn ck(table: u32, col: u16, row: u64) -> CellKey {
    CellKey {
        table: t(table),
        col: c(col),
        row: r(row),
    }
}

pub(super) fn u64_schema(table: u32, cols: &[u16]) -> TableSchema {
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

pub(super) fn schemas(list: Vec<TableSchema>) -> BTreeMap<TableId, TableSchema> {
    list.into_iter().map(|s| (s.id, s)).collect()
}

pub(super) fn empty_column_state(
    vc: &HybridVC<MockFieldHasher>,
    table: u32,
    col: u16,
) -> ((TableId, ColId), ColumnState<MockFieldHasher>) {
    let (state, _) = vc.commit_column(t(table), c(col), vec![]).unwrap();
    ((t(table), c(col)), state)
}

pub(super) fn column_state_with(
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
    let (state, _) = vc.commit_column(t(table), c(col), enc).unwrap();
    ((t(table), c(col)), state)
}

pub(super) fn read_event(
    table: u32,
    col: u16,
    row: u64,
    val: u64,
    time: u64,
    tx: u32,
) -> ExecutionEvent {
    ExecutionEvent {
        key: ck(table, col, row),
        op: OpKind::Read,
        value: Value::U64(val),
        val_is_null: false,
        time,
        tx_index: tx,
        effect_ordinal_in_tx: time as u32,
    }
}

pub(super) fn write_event(
    table: u32,
    col: u16,
    row: u64,
    val: u64,
    time: u64,
    tx: u32,
) -> ExecutionEvent {
    ExecutionEvent {
        key: ck(table, col, row),
        op: OpKind::Write,
        value: Value::U64(val),
        val_is_null: false,
        time,
        tx_index: tx,
        effect_ordinal_in_tx: time as u32,
    }
}

pub(super) fn null_read_event(
    table: u32,
    col: u16,
    row: u64,
    time: u64,
    tx: u32,
) -> ExecutionEvent {
    ExecutionEvent {
        key: ck(table, col, row),
        op: OpKind::Read,
        value: Value::U64(0),
        val_is_null: true,
        time,
        tx_index: tx,
        effect_ordinal_in_tx: time as u32,
    }
}
