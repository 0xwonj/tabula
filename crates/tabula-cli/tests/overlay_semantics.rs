//! Integration tests: overlay semantics end-to-end.

use tabula_commitment::mock::InMemoryState;
use tabula_core::types::*;
use tabula_executor::overlay::Overlay;

#[test]
fn test_overlay_read_your_writes_end_to_end() {
    let mut state = InMemoryState::new();
    let k = CellKey {
        table: TableId(1),
        col: ColId(0),
        row: RowKey(0),
    };
    state.set(k, Value::U64(100));

    let mut ov = Overlay::new(&state);

    // Read from snapshot
    let v1 = ov.read(&k, ValueType::U64).unwrap();
    assert_eq!(v1, Some(Value::U64(100)));

    // Write and read back
    ov.write(&k, Some(Value::U64(200)), ValueType::U64);
    let v2 = ov.read(&k, ValueType::U64).unwrap();
    assert_eq!(v2, Some(Value::U64(200)));

    // Finalize
    let result = ov.into_result();
    assert_eq!(result.read_set_old.len(), 1);
    assert_eq!(result.read_set_old[0], (k, Some(Value::U64(100))));
    assert_eq!(result.write_set_final.len(), 1);
    assert_eq!(result.write_set_final[0], (k, Some(Value::U64(200))));
}

#[test]
fn test_overlay_checkpoint_rollback_end_to_end() {
    let mut state = InMemoryState::new();
    let k1 = CellKey {
        table: TableId(1),
        col: ColId(0),
        row: RowKey(0),
    };
    let k2 = CellKey {
        table: TableId(1),
        col: ColId(0),
        row: RowKey(1),
    };
    state.set(k1, Value::U64(100));
    state.set(k2, Value::U64(200));

    let mut ov = Overlay::new(&state);

    // Tx 1: write to k1
    ov.write(&k1, Some(Value::U64(50)), ValueType::U64);
    ov.checkpoint();

    // Tx 2: write to k2 (will be rolled back)
    ov.write(&k2, Some(Value::U64(999)), ValueType::U64);
    ov.rollback();

    let result = ov.into_result();
    // k1 should be written, k2 should NOT
    assert_eq!(result.write_set_final.len(), 1);
    assert_eq!(result.write_set_final[0], (k1, Some(Value::U64(50))));
}

#[test]
fn test_overlay_write_coalescing_end_to_end() {
    let state = InMemoryState::new();
    let k = CellKey {
        table: TableId(1),
        col: ColId(0),
        row: RowKey(0),
    };

    let mut ov = Overlay::new(&state);
    ov.write(&k, Some(Value::U64(1)), ValueType::U64);
    ov.write(&k, Some(Value::U64(2)), ValueType::U64);
    ov.write(&k, Some(Value::U64(3)), ValueType::U64);

    let result = ov.into_result();
    assert_eq!(result.write_set_final.len(), 1);
    assert_eq!(result.write_set_final[0], (k, Some(Value::U64(3))));
}
