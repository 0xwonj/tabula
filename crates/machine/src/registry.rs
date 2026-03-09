//! Runtime chip registry for dynamic chip composition.
//!
//! [`ChipRegistry`] stores [`Box<dyn AnyRap>`] instances keyed by [`ChipId`],
//! enabling runtime composition of chip sets without compile-time enum wiring.
//!
//! Use [`core_chips()`] for the 8 Layer 0 chips, [`default_commitment_chips()`]
//! for commitment-layer chips, or the layered registration methods for finer control.

use tabula_chips::poseidon::PoseidonChip;
use tabula_chips::range_check::RangeCheckChip;
use tabula_stark::chips::ChipId;

use crate::AnyRap;
use crate::composition::{
    CommitmentScheme, GlobalSortedMemory, MemoryModel, RootProof, SmtRootProof, SsmcScheme,
    execution_airs,
};

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

    /// Register a batch of boxed chips.
    pub(crate) fn register_boxed(&mut self, chips: Vec<Box<dyn AnyRap>>) {
        for chip in chips {
            let chip_id = tabula_stark::chips::ChipSpec::chip_id(chip.as_ref());
            self.chips.push(RegisteredChip {
                chip_id,
                air: chip,
            });
        }
    }

    /// Register all 9 default Tabula chips (8 Layer 0 core + default commitments).
    ///
    /// Convenience method equivalent to calling [`register_execution`],
    /// [`register_memory_model`], [`register_root_proof`],
    /// [`register_bus_consumers`], and then registering default commitments.
    ///
    /// For Layer 0 only (without commitment chips), use
    /// [`MachineBuilder::with_core_chips()`](crate::MachineBuilder::with_core_chips).
    pub fn register_all_defaults(&mut self) {
        self.register_boxed(core_chips());
        self.register_boxed(default_commitment_chips());
    }

    /// Register execution-layer chips (ExecutionChip, StaticTableChip).
    pub(crate) fn register_execution(&mut self) {
        self.register_boxed(execution_airs());
    }

    /// Register memory model chips via a [`MemoryModel`] implementation.
    pub(crate) fn register_memory_model(&mut self, model: &dyn MemoryModel) {
        self.register_boxed(model.airs());
    }

    /// Register root proof chips via a [`RootProof`] implementation.
    pub(crate) fn register_root_proof(&mut self, proof: &dyn RootProof) {
        self.register_boxed(proof.airs());
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

impl Default for ChipRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Layer 0 core chips as boxed [`AnyRap`] trait objects.
///
/// Returns the 8 chips that form Tabula's fixed identity:
/// 1. Execution: ExecutionChip, StaticTableChip
/// 2. Memory: InterTxOrderChip (GlobalSortedMemory)
/// 3. Root proof: ColumnMetaChip, SmtColPathChip, SmtTablePathChip
/// 4. Bus consumers: PoseidonChip, RangeCheckChip
///
/// Commitment-layer chips (e.g., StateColumnChip for SSMC) are registered
/// separately via [`default_commitment_chips()`] or custom commitment schemes.
pub fn core_chips() -> Vec<Box<dyn AnyRap>> {
    let memory = GlobalSortedMemory;
    let root = SmtRootProof;

    let mut chips = execution_airs();
    chips.extend(memory.airs());
    chips.extend(root.airs());
    chips.push(Box::new(PoseidonChip));
    chips.push(Box::new(RangeCheckChip));
    chips
}

/// Default commitment-layer chips as boxed [`AnyRap`] trait objects.
///
/// Returns the chips needed by the default commitment schemes:
/// - SSMC: StateColumnChip (global sorted memory with hash chain commitments)
/// - SMT: No additional chip (root verification handled by Layer 0 RootProof chips)
///
/// Combined with [`core_chips()`], this gives the full 9-chip default set.
pub fn default_commitment_chips() -> Vec<Box<dyn AnyRap>> {
    SsmcScheme.airs()
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
    /// A DynChip (trace builder) has no corresponding AIR in the registry.
    ///
    /// Every chip registered for trace building must also be registered
    /// for proving/verification. Use `with_chip()` to add the AIR, or
    /// use a combined builder method like `with_bus_consumer()`.
    #[error("dyn_chip '{0}' has no corresponding AIR in the registry")]
    DynChipWithoutAir(ChipId),
}
