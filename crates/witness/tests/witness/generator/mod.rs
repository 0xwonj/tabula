use std::collections::BTreeMap;

use p3_koala_bear::KoalaBear;

use tabula_commitment::{ColumnState, KoalaBearCodec, MockFieldHasher, scheme_tags};
use tabula_core::traits::ValueCodec;
use tabula_core::{
    AccessEvent, CellKey, ColId, ColumnDef, OpKind, RowKey, TableId, TableSchema, Value, ValueType,
};

use tabula_witness::WitnessGenerator;

mod integration;
mod unit;

pub(super) fn make_wg() -> WitnessGenerator<MockFieldHasher> {
    WitnessGenerator::new(MockFieldHasher)
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
    table: u32,
    col: u16,
) -> ((TableId, ColId), ColumnState<MockFieldHasher>) {
    let (state, _) = ColumnState::commit(
        &MockFieldHasher,
        t(table),
        c(col),
        vec![],
        scheme_tags::SSMC,
    )
    .unwrap();
    ((t(table), c(col)), state)
}

pub(super) fn column_state_with(
    table: u32,
    col: u16,
    entries: &[(u64, u64)],
) -> ((TableId, ColId), ColumnState<MockFieldHasher>) {
    let codec = KoalaBearCodec;
    let enc: Vec<(RowKey, Vec<KoalaBear>)> = entries
        .iter()
        .map(|&(k, v)| (r(k), codec.encode(&Value::U64(v)).unwrap()))
        .collect();
    let (state, _) =
        ColumnState::commit(&MockFieldHasher, t(table), c(col), enc, scheme_tags::SSMC).unwrap();
    ((t(table), c(col)), state)
}

pub(super) fn read_event(
    table: u32,
    col: u16,
    row: u64,
    val: u64,
    time: u64,
    _tx: u32,
) -> AccessEvent {
    AccessEvent {
        key: ck(table, col, row),
        op: OpKind::Read,
        value: Value::U64(val),
        val_is_null: false,
        time,
        effect_ordinal_in_tx: time as u32,
    }
}

pub(super) fn write_event(
    table: u32,
    col: u16,
    row: u64,
    val: u64,
    time: u64,
    _tx: u32,
) -> AccessEvent {
    AccessEvent {
        key: ck(table, col, row),
        op: OpKind::Write,
        value: Value::U64(val),
        val_is_null: false,
        time,
        effect_ordinal_in_tx: time as u32,
    }
}

pub(super) fn null_read_event(table: u32, col: u16, row: u64, time: u64, _tx: u32) -> AccessEvent {
    AccessEvent {
        key: ck(table, col, row),
        op: OpKind::Read,
        value: Value::U64(0),
        val_is_null: true,
        time,
        effect_ordinal_in_tx: time as u32,
    }
}
