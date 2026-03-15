use tabula_core::{
    AccessEvent, BatchResult, CellKey, ColId, OpKind, RowKey, TableId, TxResult, Value,
};
use tabula_witness::witness::route::{KeyRoute, route_keys};

fn ck(table: u32, col: u16, row: u64) -> CellKey {
    CellKey {
        table: TableId(table),
        col: ColId(col),
        row: RowKey(row),
    }
}

fn read_ev(key: CellKey, time: u64) -> AccessEvent {
    AccessEvent {
        key,
        op: OpKind::Read,
        value: Value::U64(0),
        val_is_null: false,
        time,
        effect_ordinal_in_tx: time as u32,
    }
}

fn write_ev(key: CellKey, time: u64) -> AccessEvent {
    AccessEvent {
        key,
        op: OpKind::Write,
        value: Value::U64(0),
        val_is_null: false,
        time,
        effect_ordinal_in_tx: time as u32,
    }
}

fn empty_result() -> BatchResult {
    BatchResult {
        read_set_old: vec![],
        write_set_final: vec![],
        txs: vec![],
    }
}

#[test]
fn read_only_keys_no_writes() {
    let k1 = ck(1, 0, 1);
    let k2 = ck(1, 0, 2);
    let result = BatchResult {
        txs: vec![TxResult::success(
            vec![read_ev(k1, 1), read_ev(k2, 2)],
            vec![],
        )],
        ..empty_result()
    };
    let routes = route_keys(&result);
    assert_eq!(routes.len(), 2);
    assert_eq!(routes[&k1], KeyRoute::ReadOnly);
    assert_eq!(routes[&k2], KeyRoute::ReadOnly);
}

#[test]
fn sorted_memory_keys_with_writes() {
    let k1 = ck(1, 0, 1);
    let result = BatchResult {
        txs: vec![TxResult::success(
            vec![read_ev(k1, 1), write_ev(k1, 2)],
            vec![],
        )],
        write_set_final: vec![(k1, Some(Value::U64(42)))],
        ..empty_result()
    };
    let routes = route_keys(&result);
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[&k1], KeyRoute::SortedMemory);
}

#[test]
fn mixed_routing() {
    let k_read = ck(1, 0, 1);
    let k_write = ck(1, 0, 2);
    let result = BatchResult {
        txs: vec![TxResult::success(
            vec![
                read_ev(k_read, 1),
                read_ev(k_write, 2),
                write_ev(k_write, 3),
            ],
            vec![],
        )],
        write_set_final: vec![(k_write, Some(Value::U64(99)))],
        ..empty_result()
    };
    let routes = route_keys(&result);
    assert_eq!(routes.len(), 2);
    assert_eq!(routes[&k_read], KeyRoute::ReadOnly);
    assert_eq!(routes[&k_write], KeyRoute::SortedMemory);
}

#[test]
fn blind_write_routed_sorted_memory() {
    let k_blind = ck(1, 0, 5);
    let result = BatchResult {
        write_set_final: vec![(k_blind, Some(Value::U64(1)))],
        ..empty_result()
    };
    let routes = route_keys(&result);
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[&k_blind], KeyRoute::SortedMemory);
}

#[test]
fn empty_result_empty_map() {
    let routes = route_keys(&empty_result());
    assert!(routes.is_empty());
}

#[test]
fn deterministic_output() {
    let k1 = ck(1, 0, 1);
    let k2 = ck(1, 1, 2);
    let result = BatchResult {
        txs: vec![TxResult::success(
            vec![read_ev(k1, 1), read_ev(k2, 2), write_ev(k2, 3)],
            vec![],
        )],
        write_set_final: vec![(k2, Some(Value::U64(10)))],
        ..empty_result()
    };
    let r1 = route_keys(&result);
    let r2 = route_keys(&result);
    assert_eq!(r1, r2);
}

#[test]
fn read_of_written_key_is_sorted_memory() {
    let k = ck(1, 0, 1);
    let result = BatchResult {
        txs: vec![TxResult::success(
            vec![read_ev(k, 1), write_ev(k, 2)],
            vec![],
        )],
        write_set_final: vec![(k, Some(Value::U64(100)))],
        ..empty_result()
    };
    let routes = route_keys(&result);
    assert_eq!(routes[&k], KeyRoute::SortedMemory);
}

#[test]
fn multi_tx_read_only_same_key() {
    // Same key read by two different txs, no writes -> still ReadOnly.
    let k = ck(1, 0, 1);
    let ev0 = read_ev(k, 1);
    let ev1 = read_ev(k, 3);
    let result = BatchResult {
        txs: vec![
            TxResult::success(vec![ev0], vec![]),
            TxResult::success(vec![ev1], vec![]),
        ],
        ..empty_result()
    };
    let routes = route_keys(&result);
    assert_eq!(routes.len(), 1);
    assert_eq!(routes[&k], KeyRoute::ReadOnly);
}

#[test]
fn delete_routed_sorted_memory() {
    // write_set_final with None (delete) -> SortedMemory.
    let k = ck(1, 0, 1);
    let result = BatchResult {
        txs: vec![TxResult::success(
            vec![read_ev(k, 1), write_ev(k, 2)],
            vec![],
        )],
        write_set_final: vec![(k, None)],
        ..empty_result()
    };
    let routes = route_keys(&result);
    assert_eq!(routes[&k], KeyRoute::SortedMemory);
}
