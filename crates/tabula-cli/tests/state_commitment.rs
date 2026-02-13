//! Integration tests: mock state commitment and root computation.

use tabula_commitment::mock::*;
use tabula_core::traits::*;
use tabula_core::types::*;

#[test]
fn test_mock_state_root_deterministic() {
    let hasher = MockHasher;

    // Compute table commitment: H(colCom_1 || tableId || schemaHash)
    let pcs = MockPCS::new();
    let values = vec![Value::U64(100), Value::U64(200), Value::U64(300)];
    let col_com = pcs.commit(&values).unwrap();

    let table_id_bytes = 1u32.to_le_bytes();
    let schema_hash = hasher.hash(b"balances_schema");
    let table_com = hasher.hash_many(&[&col_com.to_bytes(), &table_id_bytes, &schema_hash]);

    // Global state root: H(tableCom || versionTag)
    let version_tag = b"v1";
    let state_root = hasher.hash_many(&[&table_com, version_tag]);

    // Same inputs → same root
    let state_root_2 = hasher.hash_many(&[&table_com, version_tag]);
    assert_eq!(state_root, state_root_2);
}

#[test]
fn test_state_transition_changes_root() {
    let hasher = MockHasher;
    let pcs = MockPCS::new();

    // Old state: [100, 200, 300]
    let old_values = vec![Value::U64(100), Value::U64(200), Value::U64(300)];
    let old_com = pcs.commit(&old_values).unwrap();
    let old_root = hasher.hash(&old_com.to_bytes());

    // New state: [100, 200, 350] (Charlie got 50)
    let new_values = vec![Value::U64(100), Value::U64(200), Value::U64(350)];
    let new_com = pcs.commit(&new_values).unwrap();
    let new_root = hasher.hash(&new_com.to_bytes());

    assert_ne!(
        old_root, new_root,
        "different state should produce different root"
    );
}

#[test]
fn test_pcs_open_verify_round_trip() {
    let pcs = MockPCS::new();
    let values = vec![Value::U64(10), Value::U64(20), Value::U64(30)];
    let commitment = pcs.commit(&values).unwrap();

    let (val, proof) = pcs.open(&commitment, &values, RowKey(1)).unwrap();
    assert_eq!(val, Value::U64(20));
    assert!(
        pcs.verify_open(&commitment, RowKey(1), &val, &proof)
            .unwrap()
    );
}

#[test]
fn test_batch_open() {
    let pcs = MockPCS::new();
    let values = vec![Value::U64(10), Value::U64(20), Value::U64(30)];
    let commitment = pcs.commit(&values).unwrap();

    let (vals, _proof) = pcs
        .batch_open(&commitment, &values, &[RowKey(0), RowKey(2)])
        .unwrap();
    assert_eq!(vals, vec![Value::U64(10), Value::U64(30)]);
}
