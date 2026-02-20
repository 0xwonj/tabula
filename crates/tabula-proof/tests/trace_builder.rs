#![cfg(feature = "stark")]

use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;

use tabula_commitment::{
    BabyBearCodec, ColumnMeta, CommitmentStrategy, HybridVC, MockFieldHasher, NativeDigest,
};
use tabula_core::traits::ValueCodec;
use tabula_core::{CellKey, ColId, RowKey, TableId, TxOutcome, Value};
use tabula_proof::air::chips::column_meta::air::ColumnMetaChip;
use tabula_proof::air::chips::inter_tx_order::air::InterTxOrderChip;
use tabula_proof::air::chips::state_column::air::StateColumnChip;
use tabula_proof::air::debug_check;
use tabula_proof::trace_builder::build_trace_bundle;
use tabula_proof::witness::{AccessRow, BatchWitness, ColumnWitness, InitRow, KeyRoute};

fn mk_codec() -> BabyBearCodec {
    BabyBearCodec
}

fn encode_u64(v: u64) -> Vec<BabyBear> {
    mk_codec().encode(&Value::U64(v)).expect("encode")
}

fn single_column_roots(
    vc: &HybridVC<MockFieldHasher>,
    table: TableId,
    col: ColId,
    com_old: NativeDigest,
    com_new: NativeDigest,
) -> (NativeDigest, NativeDigest) {
    let old_leaf = vc.compute_leaf(table, col, CommitmentStrategy::Ssmc, &com_old);
    let new_leaf = vc.compute_leaf(table, col, CommitmentStrategy::Ssmc, &com_new);

    let mut old_cols = BTreeMap::new();
    old_cols.insert(col, old_leaf);
    let mut new_cols = BTreeMap::new();
    new_cols.insert(col, new_leaf);

    let old_table = vc.compute_table_root(&old_cols);
    let new_table = vc.compute_table_root(&new_cols);

    let mut old_tables = BTreeMap::new();
    old_tables.insert(table, old_table);
    let mut new_tables = BTreeMap::new();
    new_tables.insert(table, new_table);

    (
        vc.compute_state_root(&old_tables),
        vc.compute_state_root(&new_tables),
    )
}

#[test]
fn trace_builder_builds_valid_memory_traces() {
    let vc = HybridVC::new(MockFieldHasher, 1024);
    let table = TableId(1);
    let col = ColId(0);

    let old_entries = vec![(RowKey(10), encode_u64(50))];
    let (old_state, com_old) = vc.commit_column(table, col, old_entries);

    let writes = vec![(RowKey(10), Some(encode_u64(75)))];
    let (new_state, com_new, merge_trace) = vc.apply_column_writes(&old_state, table, col, &writes);

    let meta = ColumnMeta {
        table,
        col,
        tag: CommitmentStrategy::Ssmc,
        com_old,
        com_new,
        is_empty_old: false,
        is_empty_new: false,
        is_touched: true,
    };

    let column_witness = ColumnWitness {
        table,
        col,
        value_type: tabula_core::ValueType::U64,
        init_rows: vec![InitRow {
            key: CellKey {
                table,
                col,
                row: RowKey(10),
            },
            value_fes: encode_u64(50),
            val_is_null: false,
        }],
        access_rows: vec![
            AccessRow {
                key: CellKey {
                    table,
                    col,
                    row: RowKey(10),
                },
                time: 0,
                is_write: false,
                value_fes: encode_u64(50),
                val_is_null: false,
                tx_index: 0,
                effect_ordinal_in_tx: 0,
            },
            AccessRow {
                key: CellKey {
                    table,
                    col,
                    row: RowKey(10),
                },
                time: 1,
                is_write: true,
                value_fes: encode_u64(75),
                val_is_null: false,
                tx_index: 0,
                effect_ordinal_in_tx: 1,
            },
        ],
        old_state,
        new_state,
        merge_trace,
        meta: meta.clone(),
    };

    let (old_state_root, new_state_root) = single_column_roots(&vc, table, col, com_old, com_new);

    let witness = BatchWitness {
        columns: vec![column_witness],
        column_metas: vec![meta],
        old_state_root,
        new_state_root,
        tx_outcomes: vec![TxOutcome::Success],
        key_routes: BTreeMap::<CellKey, KeyRoute>::new(),
    };

    let bundle = build_trace_bundle::<MockFieldHasher, 3>(&witness).expect("trace bundle");

    debug_check(&InterTxOrderChip::<3>, &bundle.inter_tx_trace).expect("inter-tx trace valid");
    debug_check(&StateColumnChip::<3>, &bundle.state_trace).expect("state trace valid");
    debug_check(&ColumnMetaChip, &bundle.column_meta_trace).expect("column-meta trace valid");

    assert_eq!(bundle.inter_tx_rows.len(), 2); // init + tx row
    assert_eq!(bundle.state_rows.len(), 1); // one key in one column
}
