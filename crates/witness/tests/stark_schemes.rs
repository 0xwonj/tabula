#![allow(missing_docs)]
use tabula_chips::shards::property::trace::{PROPERTY_READ_WITNESS_LABEL, PropertyReadRecord};
use tabula_chips::shards::shared::{SHARED_COLUMN_WITNESS_LABEL, SharedColumnWitness};
use tabula_chips::shards::smt_state::SMT_STATE_WITNESS_LABEL;
use tabula_chips::shards::smt_state::trace::SmtStateWitness;
use tabula_chips::shards::ssmc::{SSMC_WITNESS_LABEL, SsmcWitness};
use tabula_commitment::NativeDigest;
use tabula_core::{ColId, RootProfileId, TableId};
use tabula_profile::{ENCODING_U64_ID, TYPE_U64_ID};
use tabula_witness::stark::schemes::smt::{SmtProofInput, prepare_smt_proof};
use tabula_witness::stark::schemes::ssmc::{SsmcProofInput, prepare_ssmc_proof};

fn seeded_type_runtimes() -> tabula_types::TypeRuntimeRegistry {
    tabula_types::TypeRuntimeRegistry::seeded().expect("seeded type runtimes")
}

fn seeded_encoding_runtimes() -> tabula_types::EncodingRuntimeRegistry {
    tabula_types::EncodingRuntimeRegistry::seeded().expect("seeded encoding runtimes")
}

#[test]
fn prepares_an_empty_smt_column_store() {
    let type_runtimes = seeded_type_runtimes();
    let encoding_runtimes = seeded_encoding_runtimes();
    let type_runtime = type_runtimes.resolve(TYPE_U64_ID).expect("u64 runtime");
    let encoding_runtime = encoding_runtimes
        .resolve(ENCODING_U64_ID)
        .expect("u64 encoding");

    let prepared = prepare_smt_proof::<3>(&SmtProofInput {
        table: TableId(0),
        col: ColId(0),
        type_runtime: type_runtime.as_ref(),
        encoding_runtime: encoding_runtime.as_ref(),
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

    let prepared = prepare_ssmc_proof::<3>(&SsmcProofInput {
        table: TableId(0),
        col: ColId(0),
        type_runtime: type_runtime.as_ref(),
        encoding_runtime: encoding_runtime.as_ref(),
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
