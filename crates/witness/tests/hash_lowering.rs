#![allow(missing_docs)]

use p3_koala_bear::KoalaBear;

use tabula_chips::ir_hash::encode_ir_hash_payload;
use tabula_commitment::{NativeDigest, PoseidonHasher};
use tabula_core::TxTypeId;
use tabula_core::traits::Hasher;
use tabula_testing::witness::{compile_execute_context, lower_program_batch_for_harness};
use tabula_types::u64_portable;

#[test]
fn hash_lowering_matches_canonical_portable_hash_semantics() {
    let setup = compile_execute_context(
        "\
table dummy {
    amount: u64
}

tx h(a: u64, b: u64) {
    let digest = hash(a, b)
}
",
        &[],
        vec![tabula_core::Transaction {
            tx_type: TxTypeId(0),
            params: vec![u64_portable(7), u64_portable(9)],
        }],
    );
    let lowering = lower_program_batch_for_harness::<3>(&setup);
    assert_eq!(lowering.ir_hash_calls.len(), 1);

    let expected_inputs = vec![u64_portable(7), u64_portable(9)];
    let call = &lowering.ir_hash_calls[0];
    assert_eq!(call.payload, encode_ir_hash_payload(&expected_inputs));

    let expected_digest = PoseidonHasher::new().hash_ir(&expected_inputs);
    let actual_digest =
        NativeDigest(core::array::from_fn(|idx| KoalaBear::new(call.digest[idx]))).to_bytes();
    assert_eq!(actual_digest, expected_digest);
}
