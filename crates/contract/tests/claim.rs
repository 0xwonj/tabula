#![allow(missing_docs)]

use p3_koala_bear::KoalaBear;
use tabula_commitment::NativeDigest;
use tabula_contract::PublicStatement;

fn digest(seed: u32) -> NativeDigest {
    NativeDigest(core::array::from_fn(|offset| {
        KoalaBear::new(seed + offset as u32)
    }))
}

#[test]
fn public_statement_construction_is_explicitly_proof_visible() {
    let statement = PublicStatement {
        old_root: digest(1),
        new_root: digest(11),
        public_context_digest: digest(21),
        applied_tx_digest: digest(31),
        event_digest: digest(41),
    };

    assert_ne!(statement.old_root, statement.new_root);
    assert_eq!(
        statement.to_field_elements().len(),
        PublicStatement::NUM_FIELD_ELEMENTS
    );
}
