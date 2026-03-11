use std::collections::{BTreeMap, BTreeSet};

use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_chips::execution::{InstructionRecord, Opcode};
use tabula_chips::poseidon::constants::poseidon2_permutation;
use tabula_chips::smt_path::trace::{SmtPathWitness, SmtTablePathWitness};
use tabula_commitment::{
    BabyBearCodec, ColumnMeta, DOMAIN_COL, DOMAIN_TABLE, HybridVC, MockFieldHasher, NativeDigest,
    PoseidonHasher, SparseMerkleTree, scheme_tags,
};
use tabula_core::mock::{InMemoryState, InMemoryStaticTables, MockSigVerifier, SequentialNonce};
use tabula_core::traits::ValueCodec;
use tabula_core::{
    Batch, CellKey, ColId, RowKey, TableId, Transaction, TxOutcome, TxTypeId, Value,
};
use tabula_executor::batch::{BatchEnv, execute_batch};
use tabula_ir::Program;
use tabula_lang::compile;
use tabula_witness::WitnessGenerator;
use tabula_witness::trace::{
    AllTraceInputs, TraceBuilder, lower_execution_records, lower_program_batch,
};
use tabula_witness::witness::{AccessRow, BatchWitness, ColumnWitness, InitRow, KeyRoute};

pub(super) type EncodedColumnEntries = BTreeMap<(TableId, ColId), Vec<(RowKey, Vec<BabyBear>)>>;

pub(super) fn mk_codec() -> BabyBearCodec {
    BabyBearCodec
}

pub(super) fn encode_u64(v: u64) -> Vec<BabyBear> {
    mk_codec().encode(&Value::U64(v)).expect("encode")
}

pub(super) fn single_column_roots(
    vc: &HybridVC<MockFieldHasher>,
    table: TableId,
    col: ColId,
    com_old: NativeDigest,
    com_new: NativeDigest,
) -> (NativeDigest, NativeDigest) {
    let old_leaf = vc.compute_leaf(table, col, scheme_tags::SSMC, &com_old);
    let new_leaf = vc.compute_leaf(table, col, scheme_tags::SSMC, &com_new);

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

pub(super) fn chain_commit_single(
    table: u32,
    col: u16,
    key: u64,
    value: &[BabyBear],
) -> NativeDigest {
    const MASK_30: u64 = (1u64 << 30) - 1;
    let mut input = [BabyBear::ZERO; 16];
    input[1] = BabyBear::new(table);
    input[2] = BabyBear::new(col as u32);
    input[3] = BabyBear::new((key & MASK_30) as u32);
    input[4] = BabyBear::new(((key >> 30) & MASK_30) as u32);
    input[5] = BabyBear::new((key >> 60) as u32);
    for (i, v) in value.iter().enumerate().take(3) {
        input[6 + i] = *v;
    }
    let (_, out) = poseidon2_permutation(input);
    NativeDigest(core::array::from_fn(|i| out[i]))
}

pub(super) fn poseidon_compress(left: &NativeDigest, right: &NativeDigest) -> NativeDigest {
    let mut perm_input = [BabyBear::ZERO; 16];
    perm_input[..8].copy_from_slice(&left.0);
    perm_input[8..16].copy_from_slice(&right.0);
    let (_rounds, out) = poseidon2_permutation(perm_input);
    NativeDigest(core::array::from_fn(|i| out[i]))
}

pub(super) fn compute_leaf_digest(
    table: u32,
    col: u16,
    tag: u32,
    com: &NativeDigest,
) -> NativeDigest {
    let mut perm_input = [BabyBear::ZERO; 16];
    perm_input[0] = BabyBear::new(0x10);
    perm_input[1] = BabyBear::new(table);
    perm_input[2] = BabyBear::new(col as u32);
    perm_input[3] = BabyBear::new(tag);
    perm_input[8..16].copy_from_slice(&com.0);
    let (_rounds, out) = poseidon2_permutation(perm_input);
    NativeDigest(core::array::from_fn(|i| out[i]))
}

pub(super) fn zero_siblings(depth: usize) -> Vec<NativeDigest> {
    vec![NativeDigest::ZERO; depth]
}

pub(super) fn path_bits_from_key(key: u64, depth: usize) -> Vec<bool> {
    (0..depth).map(|i| ((key >> i) & 1) == 1).collect()
}

pub(super) fn build_smt_paths_from_metas(
    metas: &[ColumnMeta],
    old_root: &NativeDigest,
    new_root: &NativeDigest,
) -> (Vec<SmtPathWitness>, Vec<SmtTablePathWitness>) {
    const COL_DEPTH: usize = 16;
    const TABLE_DEPTH: usize = 30;

    let hasher = PoseidonHasher::new();

    let mut by_table: BTreeMap<TableId, Vec<&ColumnMeta>> = BTreeMap::new();
    for meta in metas {
        by_table.entry(meta.table).or_default().push(meta);
    }

    let mut col_paths = Vec::new();
    let mut old_table_roots = BTreeMap::new();
    let mut new_table_roots = BTreeMap::new();
    let mut root_mults = BTreeMap::new();

    for (table, metas_for_table) in &by_table {
        let mut old_tree = SparseMerkleTree::new(hasher.clone(), COL_DEPTH, DOMAIN_COL);
        let mut new_tree = SparseMerkleTree::new(hasher.clone(), COL_DEPTH, DOMAIN_COL);

        for meta in metas_for_table {
            let old_leaf =
                compute_leaf_digest(meta.table.0, meta.col.0, meta.tag as u32, &meta.com_old);
            let new_leaf =
                compute_leaf_digest(meta.table.0, meta.col.0, meta.tag as u32, &meta.com_new);

            old_tree.insert(meta.col.0 as u64, old_leaf);
            new_tree.insert(meta.col.0 as u64, new_leaf);
        }

        for meta in metas_for_table {
            let old_leaf =
                compute_leaf_digest(meta.table.0, meta.col.0, meta.tag as u32, &meta.com_old);
            let new_leaf =
                compute_leaf_digest(meta.table.0, meta.col.0, meta.tag as u32, &meta.com_new);

            let old_proof = old_tree.prove(meta.col.0 as u64);
            let new_proof = new_tree.prove(meta.col.0 as u64);
            col_paths.push(SmtPathWitness {
                table_id: table.0,
                key: meta.col.0 as u32,
                old_leaf,
                new_leaf,
                old_siblings: old_proof.siblings,
                new_siblings: new_proof.siblings,
                path_bits: path_bits_from_key(meta.col.0 as u64, COL_DEPTH),
            });
        }

        old_table_roots.insert(*table, old_tree.root());
        new_table_roots.insert(*table, new_tree.root());
        root_mults.insert(*table, metas_for_table.len() as u32);
    }

    let mut old_state_tree = SparseMerkleTree::new(hasher.clone(), TABLE_DEPTH, DOMAIN_TABLE);
    let mut new_state_tree = SparseMerkleTree::new(hasher, TABLE_DEPTH, DOMAIN_TABLE);
    for (&table, &root) in &old_table_roots {
        old_state_tree.insert(table.0 as u64, root);
    }
    for (&table, &root) in &new_table_roots {
        new_state_tree.insert(table.0 as u64, root);
    }

    assert_eq!(
        old_state_tree.root(),
        *old_root,
        "constructed old state root must match witness root"
    );
    assert_eq!(
        new_state_tree.root(),
        *new_root,
        "constructed new state root must match witness root"
    );

    let mut table_paths = Vec::new();
    for (&table, &root_mult) in &root_mults {
        let old_leaf = old_table_roots[&table];
        let new_leaf = new_table_roots[&table];
        let old_proof = old_state_tree.prove(table.0 as u64);
        let new_proof = new_state_tree.prove(table.0 as u64);
        table_paths.push(SmtTablePathWitness {
            path: SmtPathWitness {
                table_id: table.0,
                key: table.0,
                old_leaf,
                new_leaf,
                old_siblings: old_proof.siblings,
                new_siblings: new_proof.siblings,
                path_bits: path_bits_from_key(table.0 as u64, TABLE_DEPTH),
            },
            root_mult,
        });
    }

    (col_paths, table_paths)
}

mod ir_lowering;
mod memory;
