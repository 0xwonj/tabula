//! Registry of [`ChipWitnessKit`] instances driven by
//! [`prepare_execution_store`](super::prepare_execution_store).
//!
//! The runtime populates this registry during proving setup by
//! collecting each configured [`tabula_ext::backend::ExecutionBackend`]'s
//! `witness_kits()`; the witness lowering driver then iterates the
//! kits and invokes `finalize` in registration order so every kit
//! publishes its rows under its declared witness-store label.
//!
//! The registry lives in `tabula-witness` (rather than in the chip
//! registry owned by `tabula-machine`) because kits are a witness-tier
//! concern: the machine-tier `ChipRegistry` tracks AIRs for trace
//! generation, while kits sit alongside execution-store assembly.

use tabula_stark::witness_kit::ChipWitnessKit;

/// Ordered list of kits driven during execution-store assembly.
///
/// Insertion order is preserved; iteration order matches registration
/// order. Kit drive-order must be deterministic — it affects the
/// sequence of `store.put` calls that follow the kit invocations.
#[derive(Default)]
pub struct ChipKitRegistry {
    kits: Vec<Box<dyn ChipWitnessKit>>,
}

impl ChipKitRegistry {
    /// Construct an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register one kit at the back of the drive order.
    pub fn register(&mut self, kit: Box<dyn ChipWitnessKit>) {
        self.kits.push(kit);
    }

    /// Extend the drive order with multiple kits preserving the
    /// supplied iteration order.
    pub fn register_all<I>(&mut self, kits: I)
    where
        I: IntoIterator<Item = Box<dyn ChipWitnessKit>>,
    {
        self.kits.extend(kits);
    }

    /// Iterate the kits in drive order.
    pub fn iter(&self) -> impl Iterator<Item = &dyn ChipWitnessKit> {
        self.kits.iter().map(|k| k.as_ref())
    }

    /// `true` when no kits are registered.
    pub fn is_empty(&self) -> bool {
        self.kits.is_empty()
    }
}
