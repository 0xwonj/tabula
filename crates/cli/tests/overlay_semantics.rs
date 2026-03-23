//! Integration tests: overlay semantics end-to-end.

use std::sync::OnceLock;

use tabula_core::InMemoryState;
use tabula_core::{CellKey, ColId, RowKey, TableId};
use tabula_executor::overlay::Overlay;
use tabula_types::{TypeRuntimeRegistry, u64_portable, u64_typed};

fn type_runtimes() -> &'static TypeRuntimeRegistry {
    static TYPE_RUNTIMES: OnceLock<TypeRuntimeRegistry> = OnceLock::new();
    TYPE_RUNTIMES.get_or_init(|| TypeRuntimeRegistry::seeded().expect("seeded type runtimes"))
}

fn u64_type_id() -> tabula_core::TypeId {
    u64_typed(0).type_id()
}

#[test]
fn test_overlay_read_your_writes_end_to_end() {
    let mut state = InMemoryState::new();
    let k = CellKey {
        table: TableId(1),
        col: ColId(0),
        row: RowKey(0),
    };
    state.set(k, u64_portable(100));

    let mut ov = Overlay::new(&state, type_runtimes());

    let v1 = ov.read(&k, u64_type_id()).unwrap();
    assert_eq!(v1, Some(u64_typed(100)));

    ov.write(&k, Some(u64_typed(200)), u64_type_id()).unwrap();
    let v2 = ov.read(&k, u64_type_id()).unwrap();
    assert_eq!(v2, Some(u64_typed(200)));

    let result = ov.into_result().unwrap();
    assert_eq!(result.read_set_old.len(), 1);
    assert_eq!(result.read_set_old[0], (k, Some(u64_portable(100))));
    assert_eq!(result.write_set_final.len(), 1);
    assert_eq!(result.write_set_final[0], (k, Some(u64_portable(200))));
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
    state.set(k1, u64_portable(100));
    state.set(k2, u64_portable(200));

    let mut ov = Overlay::new(&state, type_runtimes());

    ov.write(&k1, Some(u64_typed(50)), u64_type_id()).unwrap();
    ov.checkpoint();

    ov.write(&k2, Some(u64_typed(999)), u64_type_id()).unwrap();
    ov.rollback();

    let result = ov.into_result().unwrap();
    assert_eq!(result.write_set_final.len(), 1);
    assert_eq!(result.write_set_final[0], (k1, Some(u64_portable(50))));
}

#[test]
fn test_overlay_write_coalescing_end_to_end() {
    let state = InMemoryState::new();
    let k = CellKey {
        table: TableId(1),
        col: ColId(0),
        row: RowKey(0),
    };

    let mut ov = Overlay::new(&state, type_runtimes());
    ov.write(&k, Some(u64_typed(1)), u64_type_id()).unwrap();
    ov.write(&k, Some(u64_typed(2)), u64_type_id()).unwrap();
    ov.write(&k, Some(u64_typed(3)), u64_type_id()).unwrap();

    let result = ov.into_result().unwrap();
    assert_eq!(result.write_set_final.len(), 1);
    assert_eq!(result.write_set_final[0], (k, Some(u64_portable(3))));
}
