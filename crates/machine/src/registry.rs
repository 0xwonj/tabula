//! Runtime chip registry for dynamic chip composition.
//!
//! [`ChipRegistry`] stores [`Box<dyn AnyRap>`] instances keyed by [`ChipId`],
//! enabling runtime composition of chip sets without compile-time enum wiring.

use tabula_chips::poseidon::PoseidonChip;
use tabula_chips::range_check::RangeCheckChip;
use tabula_stark::chips::ChipId;

use crate::AnyRap;
use crate::composition::execution_airs;

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

impl Default for ChipRegistry {
    fn default() -> Self {
        Self::new()
    }
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

    /// Register a batch of boxed chips.
    pub(crate) fn register_boxed(&mut self, chips: Vec<Box<dyn AnyRap>>) {
        for chip in chips {
            let chip_id = tabula_stark::chips::ChipSpec::chip_id(chip.as_ref());
            self.chips.push(RegisteredChip { chip_id, air: chip });
        }
    }

    /// Register execution-layer chips (ExecutionChip, StaticTableChip).
    pub(crate) fn register_execution(&mut self) {
        self.register_boxed(execution_airs());
    }

    /// Register bus consumer chips (PoseidonChip, RangeCheckChip).
    pub(crate) fn register_bus_consumers(&mut self) {
        self.register(PoseidonChip);
        self.register(RangeCheckChip);
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

/// Errors during machine setup.
#[derive(Debug, thiserror::Error)]
pub enum SetupError {
    /// A chip with the same ID was registered more than once.
    #[error("duplicate chip id: {0}")]
    DuplicateChipId(ChipId),
    /// No chips were registered before building the machine.
    #[error("no chips registered")]
    EmptyRegistry,
    /// Setup failed during tier construction.
    #[error("setup failed: {0}")]
    SetupFailed(String),
}
