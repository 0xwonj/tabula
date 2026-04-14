//! Tests for the SMT-backed per-column state shard AIR.

use std::collections::{BTreeMap, BTreeSet};

use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_chips::execution::u64_to_native_key_payload;
use tabula_chips::shards::smt_state::air::SmtStateShardChip;
use tabula_chips::shards::smt_state::trace::{
    SmtStatePathWitness, SmtStateWitness, generate_smt_state_shard_trace,
};
use tabula_commitment::primitives::{COL_DATA_SMT_DEPTH, DOMAIN_SMT};
use tabula_commitment::schemes::smt::SparseMerkleTree;
use tabula_commitment::{FieldHasher, NativeDigest, PoseidonHasher};
use tabula_stark::chips::ChipId;
use tabula_stark::debug::debug_check;

const W: usize = 3;

fn chip() -> SmtStateShardChip<W> {
    SmtStateShardChip::new(ChipId(140), 0, 0)
}

fn trace(witness: &SmtStateWitness<W>) -> RowMajorMatrix<KoalaBear> {
    generate_smt_state_shard_trace(witness)
}

fn kb(vals: [u32; W]) -> [KoalaBear; W] {
    vals.map(KoalaBear::new)
}

fn path_bits_from_key(key: u64) -> Vec<bool> {
    (0..COL_DATA_SMT_DEPTH)
        .map(|level| ((key >> level) & 1) == 1)
        .collect()
}

fn digest_value(hasher: &PoseidonHasher, value: &[KoalaBear; W]) -> NativeDigest {
    hasher.hash(value)
}

fn build_witness(
    old_entries: &[(u64, [u32; W])],
    new_entries: &[(u64, [u32; W])],
    touched_keys: &[u64],
    write_keys: &[u64],
) -> SmtStateWitness<W> {
    let hasher = PoseidonHasher::new();
    let mut old_tree = SparseMerkleTree::new(hasher.clone(), COL_DATA_SMT_DEPTH, DOMAIN_SMT);
    let mut new_tree = SparseMerkleTree::new(hasher.clone(), COL_DATA_SMT_DEPTH, DOMAIN_SMT);

    let old_map: BTreeMap<u64, [KoalaBear; W]> = old_entries
        .iter()
        .map(|(key, value)| (*key, kb(*value)))
        .collect();
    let new_map: BTreeMap<u64, [KoalaBear; W]> = new_entries
        .iter()
        .map(|(key, value)| (*key, kb(*value)))
        .collect();

    for (&key, value) in &old_map {
        old_tree.insert(key, digest_value(&hasher, value)).unwrap();
    }
    for (&key, value) in &new_map {
        new_tree.insert(key, digest_value(&hasher, value)).unwrap();
    }

    let write_key_set: BTreeSet<u64> = write_keys.iter().copied().collect();
    let paths = touched_keys
        .iter()
        .map(|key| {
            let old_proof = old_tree.prove(*key).unwrap();
            let new_proof = new_tree.prove(*key).unwrap();
            let old_val = old_map.get(key).copied().unwrap_or([KoalaBear::ZERO; W]);
            let new_val = new_map.get(key).copied().unwrap_or([KoalaBear::ZERO; W]);

            SmtStatePathWitness {
                key: u64_to_native_key_payload(*key),
                old_val,
                new_val,
                old_is_null: !old_map.contains_key(key),
                new_is_null: !new_map.contains_key(key),
                write_mult: write_key_set.contains(key),
                old_siblings: old_proof.siblings,
                new_siblings: new_proof.siblings,
                path_bits: path_bits_from_key(*key),
            }
        })
        .collect();

    SmtStateWitness {
        table_id: 0,
        col_id: 0,
        column_old_root: old_tree.root(),
        column_new_root: new_tree.root(),
        column_is_empty_old: old_map.is_empty(),
        column_is_empty_new: new_map.is_empty(),
        column_is_touched: !write_key_set.is_empty(),
        paths,
    }
}

#[test]
fn valid_membership_read_only_path() {
    let witness = build_witness(&[(7, [11, 0, 0])], &[(7, [11, 0, 0])], &[7], &[]);
    debug_check(&chip(), &trace(&witness)).expect("membership read-only path should pass");
}

#[test]
fn valid_non_membership_read_only_path() {
    let witness = build_witness(&[(7, [11, 0, 0])], &[(7, [11, 0, 0])], &[9], &[]);
    debug_check(&chip(), &trace(&witness)).expect("non-membership read-only path should pass");
}

#[test]
fn valid_insert_path() {
    let witness = build_witness(&[], &[(5, [22, 0, 0])], &[5], &[5]);
    debug_check(&chip(), &trace(&witness)).expect("insert path should pass");
}

#[test]
fn valid_update_path() {
    let witness = build_witness(&[(5, [22, 0, 0])], &[(5, [33, 0, 0])], &[5], &[5]);
    debug_check(&chip(), &trace(&witness)).expect("update path should pass");
}

#[test]
fn valid_delete_path() {
    let witness = build_witness(&[(5, [22, 0, 0])], &[], &[5], &[5]);
    debug_check(&chip(), &trace(&witness)).expect("delete path should pass");
}

#[test]
fn valid_delete_last_key_to_empty_column() {
    let witness = build_witness(&[(5, [22, 0, 0])], &[], &[5], &[5]);
    assert!(witness.column_is_empty_new);
    debug_check(&chip(), &trace(&witness)).expect("deleting the last key should pass");
}

#[test]
fn valid_untouched_column_trivial_trace() {
    let witness = build_witness(&[], &[], &[], &[]);
    debug_check(&chip(), &trace(&witness))
        .expect("untouched SMT column should emit a trivial trace");
}

#[test]
fn invalid_new_root_mismatch_fails() {
    let mut witness = build_witness(&[(5, [22, 0, 0])], &[(5, [33, 0, 0])], &[5], &[5]);
    witness.column_new_root = witness.column_old_root;
    debug_check(&chip(), &trace(&witness)).expect_err("wrong new root must fail");
}
