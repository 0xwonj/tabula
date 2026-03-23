#![allow(missing_docs)]

use p3_koala_bear::KoalaBear;

use std::collections::{BTreeMap, BTreeSet};

use tabula_chips::ir_hash::encode_ir_hash_payload;
use tabula_commitment::{NativeDigest, PoseidonHasher};
use tabula_core::InMemoryStaticTables;
use tabula_core::traits::Hasher;
use tabula_core::{Batch, TableId, TxTypeId};
use tabula_testing::exec::{compiled_program_from_source, execute_batch_with_defaults};
use tabula_types::{EncodingRuntimeRegistry, TypeRuntimeRegistry, u64_portable};

#[test]
fn hash_lowering_matches_canonical_portable_hash_semantics() {
    let compiled = compiled_program_from_source(
        "\
table dummy {
    amount: u64
}

tx h(a: u64, b: u64) {
    let digest = hash(a, b)
}
",
    );
    let batch = Batch {
        transactions: vec![tabula_core::Transaction {
            tx_type: TxTypeId(0),
            params: vec![u64_portable(7), u64_portable(9)],
            sender: [1u8; 32],
            nonce: 0,
            signature: vec![],
        }],
    };
    let result = execute_batch_with_defaults(
        &batch,
        compiled.program(),
        &tabula_core::InMemoryState::new(),
    )
    .expect("execute batch");
    let schemas = compiled
        .table_schemas()
        .iter()
        .cloned()
        .map(|schema| (schema.id, schema))
        .collect::<BTreeMap<TableId, tabula_core::TableSchema>>();
    let type_runtimes = TypeRuntimeRegistry::seeded().expect("seeded type runtimes");
    let encoding_runtimes = EncodingRuntimeRegistry::seeded().expect("seeded encoding runtimes");
    let lowering = tabula_witness::stark::lower_program_batch::<3>(
        tabula_witness::stark::LowerProgramBatchInput {
            program: compiled.program(),
            batch: &batch,
            result: &result,
            schemas: &schemas,
            type_runtimes: &type_runtimes,
            encoding_runtimes: &encoding_runtimes,
            static_tables: &InMemoryStaticTables::new(),
            empty_columns: &BTreeSet::new(),
        },
    )
    .expect("lower hash batch");
    assert_eq!(lowering.ir_hash_calls.len(), 1);

    let expected_inputs = vec![u64_portable(7), u64_portable(9)];
    let call = &lowering.ir_hash_calls[0];
    assert_eq!(call.payload, encode_ir_hash_payload(&expected_inputs));

    let expected_digest = PoseidonHasher::new().hash_ir(&expected_inputs);
    let actual_digest =
        NativeDigest(core::array::from_fn(|idx| KoalaBear::new(call.digest[idx]))).to_bytes();
    assert_eq!(actual_digest, expected_digest);
}
