//! Runtime chip registry for dynamic chip composition.
//!
//! [`ChipRegistry`] stores [`Box<dyn AnyRap>`] instances keyed by [`ChipId`],
//! enabling runtime composition of chip sets without compile-time enum wiring.
//!
//! Use [`core_chips()`] to get all 9 core Tabula chips as boxed trait objects.

use tabula_chips::column_meta::ColumnMetaChip;
use tabula_chips::execution::ExecutionChip;
use tabula_chips::inter_tx_order::InterTxOrderChip;
use tabula_chips::poseidon::PoseidonChip;
use tabula_chips::range_check::RangeCheckChip;
use tabula_chips::smt_path::{SmtColPathChip, SmtTablePathChip};
use tabula_chips::state_column::StateColumnChip;
use tabula_chips::static_table::StaticTableChip;
use tabula_stark::chips::ChipId;

use crate::AnyRap;

/// A chip registered in the [`ChipRegistry`], pairing identity with a type-erased AIR.
pub struct RegisteredChip {
    chip_id: ChipId,
    air: Box<dyn AnyRap>,
}

impl RegisteredChip {
    /// The chip's identifier.
    pub fn chip_id(&self) -> ChipId {
        self.chip_id
    }

    /// The type-erased AIR trait object.
    pub fn air(&self) -> &dyn AnyRap {
        self.air.as_ref()
    }
}

/// Runtime chip registry storing type-erased AIR implementations.
///
/// Chips are stored in registration order. Duplicate [`ChipId`]s are rejected
/// at [`validate()`](Self::validate) time.
pub struct ChipRegistry {
    chips: Vec<RegisteredChip>,
}

impl ChipRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self { chips: Vec::new() }
    }

    /// Register a single chip. The chip is boxed and stored.
    pub fn register(&mut self, chip: impl AnyRap + 'static) {
        let chip_id = chip.chip_id();
        self.chips.push(RegisteredChip {
            chip_id,
            air: Box::new(chip),
        });
    }

    /// Register all 9 core Tabula chips.
    pub fn register_core(&mut self) {
        for chip in core_chips() {
            self.chips.push(RegisteredChip {
                chip_id: tabula_stark::chips::ChipSpec::chip_id(chip.as_ref()),
                air: chip,
            });
        }
    }

    /// Validate the registry: non-empty and no duplicate [`ChipId`]s.
    pub fn validate(&self) -> Result<(), SetupError> {
        if self.chips.is_empty() {
            return Err(SetupError::EmptyRegistry);
        }

        let mut seen = std::collections::BTreeSet::new();
        for chip in &self.chips {
            if !seen.insert(chip.chip_id) {
                return Err(SetupError::DuplicateChipId(chip.chip_id));
            }
        }

        Ok(())
    }

    /// Ordered list of registered chip IDs (in registration order).
    pub fn chip_ids(&self) -> Vec<ChipId> {
        self.chips.iter().map(|c| c.chip_id).collect()
    }

    /// Iterate over all registered chips.
    pub fn chips(&self) -> &[RegisteredChip] {
        &self.chips
    }

    /// Look up a chip by its [`ChipId`].
    pub fn get(&self, id: ChipId) -> Option<&dyn AnyRap> {
        self.chips
            .iter()
            .find(|c| c.chip_id == id)
            .map(|c| c.air.as_ref())
    }
}

impl Default for ChipRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// All 9 core Tabula chips as boxed [`AnyRap`] trait objects.
pub fn core_chips() -> Vec<Box<dyn AnyRap>> {
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

/// Errors during machine setup.
#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    /// A chip with the same ID was registered more than once.
    #[error("duplicate chip id: {0}")]
    DuplicateChipId(ChipId),
    /// No chips were registered before building the machine.
    #[error("no chips registered")]
    EmptyRegistry,
}
