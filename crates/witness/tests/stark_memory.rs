#![allow(missing_docs)]
use tabula_chips::smt_path::trace::{SmtPathWitness, SmtTablePathWitness};
use tabula_commitment::PoseidonHasher;
use tabula_commitment::primitives::{DOMAIN_TABLE, TABLE_STATE_SMT_DEPTH};
use tabula_commitment::schemes::smt::SparseMerkleTree;
use tabula_stark::trace::witness_labels;
use tabula_witness::stark::{SmtRootStoreContext, prepare_smt_root_store};

#[test]
fn prepares_root_store_from_the_public_stark_surface() {
    let hasher = PoseidonHasher::new();
    let empty_root =
        SparseMerkleTree::new(hasher.clone(), TABLE_STATE_SMT_DEPTH, DOMAIN_TABLE).root();
    let store = prepare_smt_root_store(
        SmtRootStoreContext::new(&[], &empty_root, &empty_root),
        hasher,
    )
    .expect("prepare root store");

    assert!(store.contains::<Vec<SmtPathWitness>>(witness_labels::SMT_COL_PATHS));
    assert!(store.contains::<Vec<SmtTablePathWitness>>(witness_labels::SMT_TABLE_PATHS));
    assert!(store.contains::<Vec<p3_koala_bear::KoalaBear>>(witness_labels::SMT_TABLE_PVS));
}
