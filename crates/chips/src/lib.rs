//! AIR chip implementations.
//!
//! Each chip is a small struct implementing `BaseAir` + `Air` + [`ChipSpec`].
//! Use [`core_dyn_chips()`] to obtain all core chips as boxed trait objects.

#[cfg(feature = "test-utils")]
pub mod test_utils;

pub mod execution;
pub mod ir_hash;
pub mod poseidon;
pub mod precompile_transcript;
pub mod range_check;
pub mod shards;
pub mod smt_path;
pub mod static_table;

// Re-export core chip identification types from tabula-stark.
pub use tabula_stark::chips::{ChipId, ChipSpec, core_chips};

use execution::ExecutionChip;
use poseidon::PoseidonChip;
use range_check::RangeCheckChip;
use smt_path::{SmtColPathChip, SmtTablePathChip};
use static_table::StaticTableChip;

// ── Default + ChipSpec for each chip ────────────────────────────────────────

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
}

// ── Dynamic chip dispatch ───────────────────────────────────────────────────

use tabula_stark::chips::DEFAULT_VALUE_WIDTH;
use tabula_stark::trace::BusConsumer;
use tabula_stark::trace::DynChip;

/// Core Tabula chips as boxed [`DynChip`] trait objects.
///
/// Returns the execution-tier + root-tier chips in canonical order.
/// Shard chips (MemoryShard, StateShard, MetaShard) are registered
/// per-column via [`column_tier_setup()`](crate) in the machine crate.
pub fn core_dyn_chips() -> Vec<Box<dyn DynChip>> {
    vec![
        Box::new(ExecutionChip::<DEFAULT_VALUE_WIDTH>),
        Box::new(PoseidonChip),
        Box::new(RangeCheckChip),
        Box::new(StaticTableChip::<DEFAULT_VALUE_WIDTH>),
        Box::new(SmtColPathChip),
        Box::new(SmtTablePathChip),
    ]
}

/// Core chips that are bus consumers (dependent on upstream interaction data).
///
/// Returns chips that implement [`BusConsumer`] for bus-driven collection.
/// Used by the orchestrator to replace hardcoded Poseidon/RangeCheck collection.
pub fn core_bus_consumers() -> Vec<Box<dyn BusConsumer>> {
    vec![Box::new(PoseidonChip), Box::new(RangeCheckChip)]
}
