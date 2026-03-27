use crate::execution::ExecutionChip;
use crate::poseidon::PoseidonChip;
use crate::range_check::RangeCheckChip;
use crate::smt_path::{SmtColPathChip, SmtTablePathChip};
use crate::static_table::StaticTableChip;
use tabula_stark::chips::{ChipId, ChipSpec, DEFAULT_VALUE_WIDTH, core_chips};
use tabula_stark::trace::{BusConsumer, DynChip};

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
        crate::poseidon::POSEIDON_PREPROCESSED_WIDTH
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

/// Core Tabula chips as boxed [`DynChip`] trait objects.
///
/// Returns the execution-tier + root-tier chips in canonical order.
/// Shard chips (MemoryShard, StateShard, MetaShard) are registered
/// per-column via `column_tier_setup()` in the machine crate.
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
pub fn core_bus_consumers() -> Vec<Box<dyn BusConsumer>> {
    vec![Box::new(PoseidonChip), Box::new(RangeCheckChip)]
}
