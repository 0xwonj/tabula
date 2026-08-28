//! Machine-only execution-tier extension framework.
//!
//! [`ExecutionTierExtension`] packages AIR chips, trace generators, and
//! dependent bus consumers into a unit that the machine builder can attach to
//! the execution tier. This is an advanced backend composition API for direct
//! machine users.
//!
//! Stable execution-tier authoring for runtime / verifier integration lives in
//! `tabula-ext::backend::execution`; runtime and verifier bridge those ext-owned
//! backends into this machine-only trait internally.
//!
//! # Bus protocol
//!
//! Custom chips communicate with core chips via LogUp buses.
//! Bus IDs 0–99 are reserved for core; extensions should use 100+.
//! Use `tabula_stark::define_bus!` to create typed bus wrappers.

use tabula_stark::trace::DynChip;
use tabula_stark::trace::column_commitment::BusConsumer;

use crate::backend::AnyRap;

/// A packaged set of backend execution-tier chips.
pub trait ExecutionTierExtension: Send + Sync {
    /// Human-readable name for this extension (e.g., `"lighter-dex"`).
    fn name(&self) -> &str;

    /// AIR implementations for proving and verification.
    ///
    /// Each AIR is registered in the execution tier's internal registry.
    /// Chips must implement [`AnyRap`] (satisfied automatically via blanket impl
    /// for any type implementing `ChipSpec + BaseAir + Air<...>`).
    fn airs(&self) -> Vec<Box<dyn AnyRap>>;

    /// Dynamic chips for trace generation.
    ///
    /// Each chip's `TraceContributor::contribute()` method is called during
    /// trace building. Chips should read from the `WitnessStore` using custom
    /// labels populated before machine trace construction.
    fn dyn_chips(&self) -> Vec<Box<dyn DynChip>>;

    /// Bus consumers for interaction-driven trace building.
    ///
    /// Consumers collect interaction data from upstream chip traces.
    /// Only needed if your extension has chips in the Dependent phase
    /// that consume bus interactions (like PoseidonChip or RangeCheckChip).
    fn bus_consumers(&self) -> Vec<Box<dyn BusConsumer>> {
        vec![]
    }
}
