#![allow(missing_docs)]

use std::collections::BTreeSet;

use tabula_core::{
    ColId, ColumnProfileId, CommittedKeyLayout, EncodingProfileId, KeyComponentSchema,
    KeyOrderingFamily, ProgramExecutionContract, ProgramMachineShape, PropertyQueryKind,
    StateColumnContract, StateContract, StateTableContract, TableId, TableKeyContract, TypeId,
};

fn key_contract(
    components: Vec<KeyComponentSchema>,
    encodings: Vec<EncodingProfileId>,
) -> TableKeyContract {
    TableKeyContract {
        components,
        component_encoding_profile_ids: encodings,
        ordering_family: KeyOrderingFamily::LexicographicByComponent,
        committed_layout: CommittedKeyLayout {
            byte_width: 8,
            fe_width: 3,
        },
    }
}

#[test]
fn test_state_table_contract_construction() {
    let table = StateTableContract {
        id: TableId(1),
        name: "balances".into(),
        key: key_contract(
            vec![KeyComponentSchema {
                symbol: "owner".into(),
                ty: TypeId(0),
            }],
            vec![EncodingProfileId(0)],
        ),
        columns: vec![StateColumnContract {
            id: ColId(0),
            name: "balance".into(),
            ty: TypeId(0),
            column_profile_id: ColumnProfileId(0),
            required_property_queries: BTreeSet::new(),
        }],
    };
    assert_eq!(table.columns.len(), 1);
    assert_eq!(table.columns[0].column_profile_id, ColumnProfileId(0));
}

#[test]
fn borsh_round_trip_program_execution_contract() {
    let contract = ProgramExecutionContract {
        state: StateContract {
            tables: vec![StateTableContract {
                id: TableId(1),
                name: "users".into(),
                key: key_contract(
                    vec![KeyComponentSchema {
                        symbol: "id".into(),
                        ty: TypeId(0),
                    }],
                    vec![EncodingProfileId(0)],
                ),
                columns: vec![
                    StateColumnContract {
                        id: ColId(0),
                        name: "balance".into(),
                        ty: TypeId(0),
                        column_profile_id: ColumnProfileId(0),
                        required_property_queries: BTreeSet::from([PropertyQueryKind::Successor]),
                    },
                    StateColumnContract {
                        id: ColId(1),
                        name: "active".into(),
                        ty: TypeId(2),
                        column_profile_id: ColumnProfileId(1),
                        required_property_queries: BTreeSet::new(),
                    },
                ],
            }],
        },
        machine_shape: ProgramMachineShape {
            max_slots: 8,
            max_key_components: 1,
            max_key_fes: 3,
        },
    };
    let bytes = borsh::to_vec(&contract).unwrap();
    let decoded: ProgramExecutionContract = borsh::from_slice(&bytes).unwrap();
    assert_eq!(contract, decoded);
}

#[test]
fn borsh_round_trip_table_key_contract() {
    let contract = TableKeyContract {
        components: vec![
            KeyComponentSchema {
                symbol: "owner".into(),
                ty: TypeId(0),
            },
            KeyComponentSchema {
                symbol: "spender".into(),
                ty: TypeId(2),
            },
        ],
        component_encoding_profile_ids: vec![EncodingProfileId(0), EncodingProfileId(2)],
        ordering_family: KeyOrderingFamily::LexicographicByComponent,
        committed_layout: CommittedKeyLayout {
            byte_width: 9,
            fe_width: 4,
        },
    };
    let bytes = borsh::to_vec(&contract).unwrap();
    let decoded: TableKeyContract = borsh::from_slice(&bytes).unwrap();
    assert_eq!(contract, decoded);
}

#[test]
fn borsh_round_trip_program_machine_shape() {
    let shape = ProgramMachineShape {
        max_slots: 8,
        max_key_components: 2,
        max_key_fes: 4,
    };
    let bytes = borsh::to_vec(&shape).unwrap();
    let decoded: ProgramMachineShape = borsh::from_slice(&bytes).unwrap();
    assert_eq!(shape, decoded);
}

#[test]
fn table_key_contracts_sort_deterministically() {
    let mut contracts = [
        key_contract(
            vec![KeyComponentSchema {
                symbol: "z".into(),
                ty: TypeId(0),
            }],
            vec![EncodingProfileId(0)],
        ),
        key_contract(
            vec![KeyComponentSchema {
                symbol: "a".into(),
                ty: TypeId(0),
            }],
            vec![EncodingProfileId(0)],
        ),
    ];
    contracts.sort();
    assert_eq!(contracts[0].components[0].symbol, "a");
    assert_eq!(contracts[1].components[0].symbol, "z");
}
