//! Program-level metadata for proof optimization (Phase 3 template chips).
//!
//! Plain data types — no logic. Populated by a future program analyzer
//! and consumed by template chip selection during witness generation.

use std::collections::{BTreeMap, BTreeSet};

use tabula_core::{ColId, RowKey, TableId, TxTypeId};

/// Recognized execution templates for specialized AIR chips.
///
/// Each variant corresponds to a fixed instruction pattern that a
/// dedicated chip can prove more efficiently than the general interpreter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TemplateId {
    /// Simple value transfer: read(src) → write(dst).
    Transfer,
    /// Read → compute → write pattern (e.g., balance update).
    ReadComputeWrite,
}

/// A cell whose key is a compile-time literal (known statically from IR).
///
/// Literal keys enable carry-column optimizations inside template chips
/// (proof-optimization-architecture.md §5).
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LiteralCell {
    /// Table identifier.
    pub table: TableId,
    /// Column identifier.
    pub col: ColId,
    /// Row key.
    pub row: RowKey,
}

/// Per-program metadata for proof optimization decisions.
///
/// Populated once per program (not per batch), used by witness generation
/// and chip selection to pick optimal proof paths.
#[derive(Clone, Debug)]
pub struct ProgramInfo {
    /// Template classification per tx-type. `None` = no template match
    /// (use general interpreter chip).
    pub tx_type_templates: BTreeMap<TxTypeId, Option<TemplateId>>,
    /// Set of cells accessed via literal keys across all tx types.
    pub literal_cells: BTreeSet<LiteralCell>,
    /// Maximum number of distinct keys accessed by any single tx.
    pub max_keys_per_tx: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
