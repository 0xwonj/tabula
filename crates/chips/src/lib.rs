#![warn(missing_docs)]
#![deny(unused)]

//! AIR chip implementations.
//!
//! Each chip is a small struct implementing `BaseAir` + `Air` + [`ChipSpec`].
//! Use [`core_dyn_chips()`] to obtain all 9 core chips as boxed trait objects.

#[cfg(feature = "test-utils")]
pub mod test_utils;

pub mod column_meta;
pub mod execution;
pub mod inter_tx_order;
pub mod poseidon;
pub mod range_check;
pub mod smt_path;
pub mod state_column;
pub mod static_table;

// Re-export core chip identification types from tabula-stark.
pub use tabula_stark::chips::{ChipId, ChipSpec, core_chips};

use column_meta::ColumnMetaChip;
use execution::ExecutionChip;
use inter_tx_order::InterTxOrderChip;
use poseidon::PoseidonChip;
use range_check::RangeCheckChip;
use smt_path::{SmtColPathChip, SmtTablePathChip};
use state_column::StateColumnChip;
use static_table::StaticTableChip;

// ── Default + ChipSpec for each chip ────────────────────────────────────────

impl Default for ColumnMetaChip {
    fn default() -> Self {
        Self
    }
}
impl ChipSpec for ColumnMetaChip {
    fn chip_id(&self) -> ChipId {
        core_chips::COLUMN_META
    }
}

impl Default for RangeCheckChip {
    fn default() -> Self {
        Self
    }
}
impl ChipSpec for RangeCheckChip {
    fn chip_id(&self) -> ChipId {
        core_chips::RANGE_CHECK
    }
    fn has_interactions(&self) -> bool {
        false
    }
}

impl Default for PoseidonChip {
    fn default() -> Self {
        Self
    }
}
impl ChipSpec for PoseidonChip {
    fn chip_id(&self) -> ChipId {
        core_chips::POSEIDON
    }
    fn preprocessed_width(&self) -> usize {
        poseidon::POSEIDON_PREPROCESSED_WIDTH
    }
}

impl<const W: usize> Default for ExecutionChip<W> {
    fn default() -> Self {
        Self
    }
}
impl<const W: usize> ChipSpec for ExecutionChip<W> {
    fn chip_id(&self) -> ChipId {
        core_chips::EXECUTION
    }
}

impl<const W: usize> Default for StateColumnChip<W> {
    fn default() -> Self {
        Self
    }
}
impl<const W: usize> ChipSpec for StateColumnChip<W> {
    fn chip_id(&self) -> ChipId {
        core_chips::STATE_COLUMN
    }
}

impl<const W: usize> Default for InterTxOrderChip<W> {
    fn default() -> Self {
        Self
    }
}
impl<const W: usize> ChipSpec for InterTxOrderChip<W> {
    fn chip_id(&self) -> ChipId {
        core_chips::INTER_TX_ORDER
    }
}

impl<const W: usize> Default for StaticTableChip<W> {
    fn default() -> Self {
        Self
    }
}
impl<const W: usize> ChipSpec for StaticTableChip<W> {
    fn chip_id(&self) -> ChipId {
        core_chips::STATIC_TABLE
    }
}

impl Default for SmtColPathChip {
    fn default() -> Self {
        Self
    }
}
impl ChipSpec for SmtColPathChip {
    fn chip_id(&self) -> ChipId {
        core_chips::SMT_COL_PATH
    }
}

impl Default for SmtTablePathChip {
    fn default() -> Self {
        Self
    }
}
impl ChipSpec for SmtTablePathChip {
    fn chip_id(&self) -> ChipId {
        core_chips::SMT_TABLE_PATH
    }
    fn num_public_values(&self) -> usize {
        smt_path::air::SMT_TABLE_PATH_NUM_PUBLIC_VALUES
    }
}

pub mod public_input;

// ── Dynamic chip dispatch ───────────────────────────────────────────────────

use tabula_stark::trace::DynChip;

/// All 9 core Tabula chips as boxed [`DynChip`] trait objects.
///
/// For use with the dynamic dispatch trace orchestration and validation pipelines.
/// Chips are returned in the canonical order matching [`core_chips::ALL`].
pub fn core_dyn_chips() -> Vec<Box<dyn DynChip>> {
    vec![
        Box::new(ExecutionChip::<3>),
        Box::new(InterTxOrderChip::<3>),
        Box::new(StateColumnChip::<3>),
        Box::new(ColumnMetaChip),
        Box::new(PoseidonChip),
        Box::new(RangeCheckChip),
        Box::new(StaticTableChip::<3>),
        Box::new(SmtColPathChip),
        Box::new(SmtTablePathChip),
    ]
}
