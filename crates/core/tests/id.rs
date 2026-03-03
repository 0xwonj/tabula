use tabula_core::{CellKey, ColId, RowKey, StateRoot, TableId, TxTypeId};

#[test]
fn cellkey_ordering() {
    let a = CellKey {
        table: TableId(1),
        col: ColId(0),
        row: RowKey(0),
    };
    let b = CellKey {
        table: TableId(1),
        col: ColId(1),
        row: RowKey(0),
    };
    let c = CellKey {
        table: TableId(1),
        col: ColId(0),
        row: RowKey(1),
    };
    let d = CellKey {
        table: TableId(2),
        col: ColId(0),
        row: RowKey(0),
    };

    assert!(a < b, "same table, col 0 < col 1");
    assert!(a < c, "same table+col, row 0 < row 1");
    assert!(c < d, "table 1 < table 2");
    assert!(
        c < b,
        "col 0 row 1 < col 1 row 0: col takes priority over row"
    );
}

#[test]
fn borsh_round_trip_cellkey() {
    let ck = CellKey {
        table: TableId(5),
        col: ColId(3),
        row: RowKey(100),
    };
    let bytes = borsh::to_vec(&ck).unwrap();
    let decoded: CellKey = borsh::from_slice(&bytes).unwrap();
    assert_eq!(ck, decoded);
}

#[test]
fn borsh_round_trip_state_root() {
    let root = StateRoot([0xAB; 32]);
    let bytes = borsh::to_vec(&root).unwrap();
    let decoded: StateRoot = borsh::from_slice(&bytes).unwrap();
    assert_eq!(root, decoded);
}

#[test]
fn display_types() {
    assert_eq!(format!("{}", TableId(5)), "table:5");
    assert_eq!(format!("{}", ColId(3)), "col:3");
    assert_eq!(format!("{}", RowKey(100)), "row:100");
    assert_eq!(format!("{}", TxTypeId(7)), "tx_type:7");
    assert_eq!(
        format!(
            "{}",
            CellKey {
                table: TableId(1),
                col: ColId(2),
                row: RowKey(3)
            }
        ),
        "(1:2:3)"
    );
}

#[test]
fn from_conversions() {
    assert_eq!(TableId::from(5u32), TableId(5));
    assert_eq!(u32::from(TableId(5)), 5);
    assert_eq!(ColId::from(3u16), ColId(3));
    assert_eq!(u16::from(ColId(3)), 3);
    assert_eq!(RowKey::from(100u64), RowKey(100));
    assert_eq!(u64::from(RowKey(100)), 100);
    assert_eq!(TxTypeId::from(7u32), TxTypeId(7));
    assert_eq!(u32::from(TxTypeId(7)), 7);
}
