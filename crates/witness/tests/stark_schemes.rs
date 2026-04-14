#![allow(missing_docs)]
use tabula_chips::shards::property::trace::{PROPERTY_READ_WITNESS_LABEL, PropertyReadRecord};
use tabula_chips::shards::shared::{SHARED_COLUMN_WITNESS_LABEL, SharedColumnWitness};
use tabula_chips::shards::smt_state::SMT_STATE_WITNESS_LABEL;
use tabula_chips::shards::smt_state::trace::SmtStateWitness;
use tabula_chips::shards::ssmc::{SSMC_WITNESS_LABEL, SsmcWitness};
use tabula_commitment::NativeDigest;
use tabula_core::{
    ColId, CommittedKey, CommittedKeyLayout, EncodingProfileId, KeyComponentSchema,
    KeyOrderingFamily, RootProfileId, TableId, TableKeyContract, TypeId,
};
use tabula_profile::{ENCODING_U64_ID, TYPE_U64_ID};
use tabula_types::{TableKeyCodec, u64_typed};
use tabula_witness::stark::schemes::smt::{SmtProofInput, prepare_smt_proof};
use tabula_witness::stark::schemes::ssmc::{SsmcProofInput, prepare_ssmc_proof};
use tabula_witness::{ColumnWrite, CommittedEntry};

fn seeded_type_runtimes() -> tabula_types::TypeRuntimeRegistry {
    tabula_types::TypeRuntimeRegistry::seeded().expect("seeded type runtimes")
}

fn seeded_encoding_runtimes() -> tabula_types::EncodingRuntimeRegistry {
    tabula_types::EncodingRuntimeRegistry::seeded().expect("seeded encoding runtimes")
}

fn u64_key_codec(encoding_runtimes: &tabula_types::EncodingRuntimeRegistry) -> TableKeyCodec {
    TableKeyCodec::from_contract(
        TableId(0),
        &TableKeyContract {
            components: vec![KeyComponentSchema {
                symbol: "id".into(),
                ty: TypeId(TYPE_U64_ID.0),
            }],
            component_encoding_profile_ids: vec![EncodingProfileId(ENCODING_U64_ID.0)],
            ordering_family: KeyOrderingFamily::LexicographicByComponent,
            committed_layout: CommittedKeyLayout {
                byte_width: 8,
                fe_width: 3,
            },
        },
        encoding_runtimes,
    )
    .expect("u64 key codec")
}

fn committed_u64_key(key_codec: &TableKeyCodec, value: u64) -> CommittedKey {
    key_codec
        .encode_tuple(&[u64_typed(value)])
        .expect("encode committed key")
}

#[test]
fn prepares_an_empty_smt_column_store() {
    let type_runtimes = seeded_type_runtimes();
    let encoding_runtimes = seeded_encoding_runtimes();
    let type_runtime = type_runtimes.resolve(TYPE_U64_ID).expect("u64 runtime");
    let encoding_runtime = encoding_runtimes
        .resolve(ENCODING_U64_ID)
        .expect("u64 encoding");
    let key_codec = u64_key_codec(&encoding_runtimes);

    let prepared = prepare_smt_proof::<3>(&SmtProofInput {
        table: TableId(0),
        col: ColId(0),
        type_runtime: type_runtime.as_ref(),
        encoding_runtime: encoding_runtime.as_ref(),
        key_codec: &key_codec,
        old_entries: &[],
        init_cells: &[],
        access_events: &[],
        writes: &[],
        is_touched: false,
        root_binding_family: RootProfileId(0),
        column_profile_hash: [0; 32],
        binding_digest: NativeDigest::ZERO,
    })
    .expect("prepare smt proof");

    assert_eq!(prepared.root_binding.table, TableId(0));
    assert_eq!(prepared.root_binding.col, ColId(0));
    assert!(
        prepared
            .store
            .contains::<SharedColumnWitness>(SHARED_COLUMN_WITNESS_LABEL)
    );
    assert!(
        prepared
            .store
            .contains::<SmtStateWitness<3>>(SMT_STATE_WITNESS_LABEL)
    );
}

#[test]
fn prepares_an_empty_ssmc_column_store_without_property_lane_data() {
    let type_runtimes = seeded_type_runtimes();
    let encoding_runtimes = seeded_encoding_runtimes();
    let type_runtime = type_runtimes.resolve(TYPE_U64_ID).expect("u64 runtime");
    let encoding_runtime = encoding_runtimes
        .resolve(ENCODING_U64_ID)
        .expect("u64 encoding");
    let key_codec = u64_key_codec(&encoding_runtimes);

    let prepared = prepare_ssmc_proof::<3>(&SsmcProofInput {
        table: TableId(0),
        col: ColId(0),
        type_runtime: type_runtime.as_ref(),
        encoding_runtime: encoding_runtime.as_ref(),
        key_codec: &key_codec,
        old_entries: &[],
        init_cells: &[],
        access_events: &[],
        writes: &[],
        is_touched: false,
        property_reads: &[],
        root_binding_family: RootProfileId(0),
        column_profile_hash: [0; 32],
        binding_digest: NativeDigest::ZERO,
    })
    .expect("prepare ssmc proof");

    assert_eq!(prepared.root_binding.table, TableId(0));
    assert_eq!(prepared.root_binding.col, ColId(0));
    assert!(
        prepared
            .store
            .contains::<SharedColumnWitness>(SHARED_COLUMN_WITNESS_LABEL)
    );
    assert!(prepared.store.contains::<SsmcWitness>(SSMC_WITNESS_LABEL));
    assert!(
        !prepared
            .store
            .contains::<Vec<PropertyReadRecord>>(PROPERTY_READ_WITNESS_LABEL)
    );
}

#[test]
fn ssmc_proof_prep_orders_keys_by_key_codec_order() {
    let type_runtimes = seeded_type_runtimes();
    let encoding_runtimes = seeded_encoding_runtimes();
    let type_runtime = type_runtimes.resolve(TYPE_U64_ID).expect("u64 runtime");
    let encoding_runtime = encoding_runtimes
        .resolve(ENCODING_U64_ID)
        .expect("u64 encoding");
    let key_codec = u64_key_codec(&encoding_runtimes);

    let small = committed_u64_key(&key_codec, 2);
    let large = committed_u64_key(&key_codec, 1 << 30);
    let small_payload = key_codec
        .encode_padded_proof_payload(&small)
        .expect("small payload");
    let large_payload = key_codec
        .encode_padded_proof_payload(&large)
        .expect("large payload");
    assert_eq!(
        key_codec
            .compare(&small, &large)
            .expect("committed-key compare"),
        std::cmp::Ordering::Less
    );
    assert_eq!(
        key_codec
            .compare_padded_payloads(&small_payload, &large_payload)
            .expect("payload compare"),
        std::cmp::Ordering::Less,
        "proof-visible payload order should match canonical key order"
    );

    let prepared = prepare_ssmc_proof::<3>(&SsmcProofInput {
        table: TableId(0),
        col: ColId(0),
        type_runtime: type_runtime.as_ref(),
        encoding_runtime: encoding_runtime.as_ref(),
        key_codec: &key_codec,
        old_entries: &[CommittedEntry {
            key: large.clone(),
            value: u64_typed(9),
            is_null: false,
        }],
        init_cells: &[],
        access_events: &[],
        writes: &[ColumnWrite {
            key: small.clone(),
            value: Some(u64_typed(2)),
        }],
        is_touched: true,
        property_reads: &[],
        root_binding_family: RootProfileId(0),
        column_profile_hash: [0; 32],
        binding_digest: NativeDigest::ZERO,
    })
    .expect("prepare ssmc proof");

    let witness = prepared
        .store
        .get::<SsmcWitness>(SSMC_WITNESS_LABEL)
        .expect("ssmc witness");
    let column = witness.get(TableId(0), ColId(0)).expect("column witness");

    assert_eq!(column.state_rows.len(), 2);
    assert_eq!(column.state_rows[0].key, small_payload);
    assert_eq!(column.state_rows[1].key, large_payload);
}
