use std::collections::BTreeMap;

use tabula_commitment::schemes::ssmc::{SsmcEntry, SsmcList};
use tabula_core::{
    AccessEvent, CellKey, ColId, ColumnDef, ColumnProfileId, OpKind, PortableValue, RowKey,
    TableId, TableSchema,
};
use tabula_profile::{
    ColumnProfile, CommitmentRole, ENCODING_U64_ID, ProfileCatalog, SCHEME_PROFILE_SSMC_ID,
    TYPE_U64_ID, builtin_catalog,
};
use tabula_types::builtins::encode_seeded_field_elements;
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry, u64_portable, u64_typed};

use tabula_witness::ExecutionInputPreparer;

mod generator;

pub(super) fn make_preparer() -> ExecutionInputPreparer {
    ExecutionInputPreparer::new()
}

pub(super) fn seeded_type_runtimes() -> TypeRuntimeRegistry {
    TypeRuntimeRegistry::seeded().expect("seeded type runtimes")
}

pub(super) fn seeded_encoding_runtimes() -> EncodingRuntimeRegistry {
    EncodingRuntimeRegistry::seeded().expect("seeded encoding runtimes")
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

pub(super) fn some(value: PortableValue) -> Option<PortableValue> {
    Some(value)
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
                column_profile_id: ColumnProfileId((table << 16) | u32::from(col)),
            })
            .collect(),
    }
}

pub(super) fn schemas(list: Vec<TableSchema>) -> BTreeMap<TableId, TableSchema> {
    list.into_iter().map(|s| (s.id, s)).collect()
}

pub(super) fn profile_catalog_for_schemas(
    schemas: &BTreeMap<TableId, TableSchema>,
) -> ProfileCatalog {
    let mut catalog = builtin_catalog().expect("built-in catalog");
    let type_descriptor = catalog
        .type_descriptor(TYPE_U64_ID)
        .cloned()
        .expect("u64 descriptor");
    let encoding_profile = catalog
        .encoding_profile(ENCODING_U64_ID)
        .cloned()
        .expect("u64 encoding");
    let scheme_profile = catalog
        .scheme_profile(SCHEME_PROFILE_SSMC_ID)
        .cloned()
        .expect("ssmc profile");
    for schema in schemas.values() {
        for column in &schema.columns {
            let column_profile = ColumnProfile::new(
                column.column_profile_id,
                format!("{}.{}", schema.name, column.name),
                None,
                &type_descriptor,
                &encoding_profile,
                &scheme_profile,
                CommitmentRole::IncludedInRoot,
            )
            .expect("column profile");
            catalog
                .register_column(column_profile)
                .expect("register column profile");
        }
    }
    catalog
}

pub(super) fn empty_column_state(table: u32, col: u16) -> ((TableId, ColId), SsmcList) {
    (
        (t(table), c(col)),
        SsmcList::from_sorted(t(table), c(col), vec![]).unwrap(),
    )
}

pub(super) fn column_state_with(
    table: u32,
    col: u16,
    entries: &[(u64, u64)],
) -> ((TableId, ColId), SsmcList) {
    let enc: Vec<_> = entries
        .iter()
        .map(|&(k, v)| SsmcEntry {
            key: r(k),
            value: encode_seeded_field_elements(&u64_typed(v)).unwrap(),
        })
        .collect();
    (
        (t(table), c(col)),
        SsmcList::from_sorted(t(table), c(col), enc).unwrap(),
    )
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
        value: u64_portable(val),
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
        value: u64_portable(val),
        val_is_null: false,
        time,
        effect_ordinal_in_tx: time as u32,
    }
}

pub(super) fn null_read_event(table: u32, col: u16, row: u64, time: u64, _tx: u32) -> AccessEvent {
    AccessEvent {
        key: ck(table, col, row),
        op: OpKind::Read,
        value: u64_portable(0),
        val_is_null: true,
        time,
        effect_ordinal_in_tx: time as u32,
    }
}
