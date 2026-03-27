#![allow(missing_docs)]
use std::collections::BTreeSet;

use tabula_chips::ir_hash::{IR_HASH_WITNESS_LABEL, IrHashCall};
use tabula_chips::relation_table::{RELATION_TABLE_WITNESS_LABEL, RelationTableWitnessRow};
use tabula_chips::relation_transcript::{
    RELATION_TRANSCRIPT_WITNESS_LABEL, RelationTranscriptCall,
};
use tabula_chips::static_table::trace::StaticTableRow;
use tabula_commitment::PoseidonHasher;
use tabula_contract::{StaticTableArtifact, TupleEncodingDefaults, TupleEncodingSelection};
use tabula_executor::TxCall;
use tabula_ir as ir;
use tabula_profile::{
    ENCODING_BOOL_ID, ENCODING_BYTES32_ID, ENCODING_U64_ID, TYPE_BOOL_ID, TYPE_BYTES32_ID,
    TYPE_U64_ID,
};
use tabula_stark::trace::witness_labels;
use tabula_witness::prepare_relation_proof;
use tabula_witness::stark::{
    LowerSuccessfulTxInput, lower_successful_tx, merge_lowering_outputs, prepare_execution_store,
};

fn empty_tx_entry() -> ir::Entry {
    ir::Entry {
        id: ir::EntryId(0),
        symbol: "noop".to_owned(),
        kind: ir::EntryKind::Tx,
        params: vec![],
        returns: vec![],
        return_policy: ir::ReturnPolicy::Unit,
        body: ir::Body {
            locals: vec![],
            ops: vec![],
        },
    }
}

fn program_with_entry(entry: ir::Entry) -> ir::Program {
    ir::Program {
        program_id: ir::ProgramId(0),
        state: ir::StateSchema { tables: vec![] },
        context: ir::ContextSchema { fields: vec![] },
        const_pool: ir::ConstantPool { entries: vec![] },
        relation_manifest: ir::RelationManifest { entries: vec![] },
        capability_manifest: ir::CapabilityManifest { entries: vec![] },
        event_manifest: ir::EventManifest { entries: vec![] },
        entries: vec![entry],
    }
}

fn empty_static_table_artifact() -> StaticTableArtifact {
    StaticTableArtifact {
        rows: vec![],
        root: [0; 32],
    }
}

fn seeded_type_runtimes() -> tabula_types::TypeRuntimeRegistry {
    tabula_types::TypeRuntimeRegistry::seeded().expect("seeded type runtimes")
}

fn seeded_encoding_runtimes() -> tabula_types::EncodingRuntimeRegistry {
    tabula_types::EncodingRuntimeRegistry::seeded().expect("seeded encoding runtimes")
}

fn tuple_encoding_defaults() -> TupleEncodingDefaults {
    TupleEncodingDefaults::new(vec![
        TupleEncodingSelection {
            type_id: TYPE_BOOL_ID,
            encoding_profile_id: ENCODING_BOOL_ID,
        },
        TupleEncodingSelection {
            type_id: TYPE_U64_ID,
            encoding_profile_id: ENCODING_U64_ID,
        },
        TupleEncodingSelection {
            type_id: TYPE_BYTES32_ID,
            encoding_profile_id: ENCODING_BYTES32_ID,
        },
    ])
    .expect("tuple encoding defaults")
}

#[test]
fn lowers_an_empty_tx_entry_and_builds_execution_store() {
    let entry = empty_tx_entry();
    let program = program_with_entry(entry.clone());
    let context = tabula_executor::ContextValues::new();
    let call = TxCall {
        entry_id: entry.id,
        params: vec![],
    };
    let empty_columns = BTreeSet::new();
    let type_runtimes = seeded_type_runtimes();
    let encoding_runtimes = seeded_encoding_runtimes();
    let tuple_encoding_defaults = tuple_encoding_defaults();
    let hasher = PoseidonHasher::new();

    let lowered = lower_successful_tx::<3>(LowerSuccessfulTxInput {
        tx_index: 0,
        program: &program,
        call: &call,
        entry: &program.entries[0],
        context: &context,
        state_effects: &[],
        event_effects: &[],
        relation_effects: &[],
        empty_columns: &empty_columns,
        type_runtimes: &type_runtimes,
        encoding_runtimes: &encoding_runtimes,
        tuple_encoding_defaults: &tuple_encoding_defaults,
        hasher: &hasher,
    })
    .expect("lower tx");

    assert!(lowered.instruction_records.is_empty());
    assert!(lowered.static_table_rows.is_empty());
    assert!(lowered.ir_hash_calls.is_empty());
    assert!(lowered.relation_transcript_calls.is_empty());
    assert!(lowered.relation_claims.is_empty());

    let merged = merge_lowering_outputs([&lowered]);
    let relation_proof = prepare_relation_proof(&program, &empty_static_table_artifact(), &[])
        .expect("prepare empty relation proof");
    let store = prepare_execution_store(&merged, &relation_proof).expect("execution store");

    assert!(
        store.contains::<Vec<tabula_chips::execution::trace::InstructionRecord>>(
            witness_labels::EXECUTION_RECORDS
        )
    );
    assert!(store.contains::<Vec<StaticTableRow>>(witness_labels::STATIC_TABLE_ROWS));
    assert!(store.contains::<Vec<IrHashCall>>(IR_HASH_WITNESS_LABEL));
    assert!(store.contains::<Vec<RelationTranscriptCall>>(RELATION_TRANSCRIPT_WITNESS_LABEL));
    assert!(store.contains::<Vec<RelationTableWitnessRow>>(RELATION_TABLE_WITNESS_LABEL));
}
