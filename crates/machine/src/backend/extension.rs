//! Extension framework for custom chip integration.
//!
//! [`ChipExtension`] packages custom AIR chips, trace generators, and bus
//! declarations into a distributable unit. Register extensions via
//! [`MachineBuilder::with_extension()`](crate::MachineBuilder).
//!
//! # Bus protocol
//!
//! Custom chips communicate with core chips via LogUp buses.
//! Bus IDs 0–99 are reserved for core; extensions should use 100+.
//! Use `tabula_stark::define_bus!` to create typed bus wrappers.

use tabula_stark::trace::DynChip;
use tabula_stark::trace::column_commitment::BusConsumer;
use tabula_stark::trace::contributor::WitnessStore;

use crate::AnyRap;

/// Context provided to extensions during witness population.
///
/// Gives extensions access to batch-level information needed to generate
/// witness data. Expanded in future phases with precompile events,
/// batch metadata, and proof plan information.
pub struct ExtensionContext {
    // Minimal for Phase 1. Future fields:
    // - precompile_events: Vec<PrecompileEvent>,
    // - batch_metadata: BatchMetadata,
    // - proof_plan: ProofPlan,
}

impl ExtensionContext {
    /// Create a new extension context.
    ///
    /// Phase 1 provides an empty context. Future phases will populate
    /// precompile events, batch metadata, and proof plan data.
    #[allow(dead_code)] // Used in Phase 2+ witness population pipeline
    pub(crate) fn new() -> Self {
        Self {}
    }
}

/// A packaged set of custom chips for Tabula's execution tier.
///
/// Extensions are the primary distribution unit for custom AIR chips.
/// Each extension provides:
/// - **AIRs**: Type-erased constraint systems for proving/verification
/// - **DynChips**: Trace generators for witness-to-trace conversion
/// - **BusConsumers** (optional): Chips that collect bus interaction data
/// - **Witness population** (optional): Logic to populate the WitnessStore
///
/// # Example
///
/// ```ignore
/// use tabula_machine::prelude::*;
///
/// struct MyExtension;
///
/// impl ChipExtension for MyExtension {
///     fn name(&self) -> &str { "my-extension" }
///     fn airs(&self) -> Vec<Box<dyn AnyRap>> { vec![Box::new(MyChip)] }
///     fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> { vec![Box::new(MyChip)] }
/// }
/// ```
pub trait ChipExtension: Send + Sync {
    /// Human-readable name for this extension (e.g., `"lighter-dex"`).
    fn name(&self) -> &str;

    /// AIR implementations for proving and verification.
    ///
    /// Each AIR is registered in the execution tier's [`ChipRegistry`](crate::ChipRegistry).
    /// Chips must implement [`AnyRap`] (satisfied automatically via blanket impl
    /// for any type implementing `ChipSpec + BaseAir + Air<...>`).
    fn airs(&self) -> Vec<Box<dyn AnyRap>>;

    /// Dynamic chips for trace generation.
    ///
    /// Each chip's [`TraceContributor::contribute()`] is called during
    /// trace building. Chips should read from the [`WitnessStore`] using
    /// custom labels populated by [`populate_witness()`](Self::populate_witness).
    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>>;

    /// Bus consumers for interaction-driven trace building.
    ///
    /// Consumers collect interaction data from upstream chip traces.
    /// Only needed if your extension has chips in the Dependent phase
    /// that consume bus interactions (like PoseidonChip or RangeCheckChip).
    fn bus_consumers(&self) -> Vec<Box<dyn BusConsumer>> {
        vec![]
    }

    /// Populate witness data for this extension's chips.
    ///
    /// Called after core witness data is populated but before trace building.
    /// Store custom data in the [`WitnessStore`] under extension-specific labels.
    fn populate_witness(&self, _store: &mut WitnessStore, _ctx: &ExtensionContext) {
        // Default: no custom witness population.
    }
}
