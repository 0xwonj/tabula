use std::collections::{BTreeMap, BTreeSet};

use tabula_core::{ColId, RowKey, TableId, TxTypeId};
use tabula_witness::program_info::{LiteralCell, ProgramInfo, TemplateId};

#[test]
fn program_info_construction() {
    let mut templates = BTreeMap::new();
    templates.insert(TxTypeId(0), Some(TemplateId::Transfer));
    templates.insert(TxTypeId(1), None);

    let mut literals = BTreeSet::new();
    literals.insert(LiteralCell {
        table: TableId(1),
        col: ColId(0),
        row: RowKey(42),
    });

    let info = ProgramInfo {
        tx_type_templates: templates,
        literal_cells: literals,
        max_keys_per_tx: 4,
    };

    assert_eq!(
        info.tx_type_templates[&TxTypeId(0)],
        Some(TemplateId::Transfer)
    );
    assert_eq!(info.tx_type_templates[&TxTypeId(1)], None);
    assert_eq!(info.literal_cells.len(), 1);
    assert_eq!(info.max_keys_per_tx, 4);
}

#[test]
fn literal_cell_ordering() {
    let a = LiteralCell {
        table: TableId(1),
        col: ColId(0),
        row: RowKey(10),
    };
    let b = LiteralCell {
        table: TableId(1),
        col: ColId(0),
        row: RowKey(20),
    };
    let c = LiteralCell {
        table: TableId(1),
        col: ColId(1),
        row: RowKey(5),
    };
    let d = LiteralCell {
        table: TableId(2),
        col: ColId(0),
        row: RowKey(1),
    };

    let mut set = BTreeSet::new();
    set.insert(d.clone());
    set.insert(b.clone());
    set.insert(a.clone());
    set.insert(c.clone());

    let ordered: Vec<_> = set.into_iter().collect();
    assert_eq!(ordered, vec![a, b, c, d]);
}
