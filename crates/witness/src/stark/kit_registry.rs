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
    ///
    /// Panics on duplicate `ChipId` or duplicate witness-store label:
    /// both are wiring bugs (two kits claiming the same store slot
    /// would silently clobber each other during finalize).
    pub fn register(&mut self, kit: Box<dyn ChipWitnessKit>) {
        let incoming_id = kit.chip_id();
        let incoming_label = kit.witness_store_label();
        for existing in &self.kits {
            assert!(
                existing.chip_id() != incoming_id,
                "duplicate ChipWitnessKit registration for chip {incoming_id}",
            );
            assert!(
                existing.witness_store_label() != incoming_label,
                "duplicate ChipWitnessKit witness-store label {incoming_label:?} \
                 (chips {} and {})",
                existing.chip_id(),
                incoming_id,
            );
        }
        self.kits.push(kit);
    }

    /// Extend the drive order with multiple kits preserving the
    /// supplied iteration order. See [`Self::register`] for the
    /// duplicate-registration invariants.
    pub fn register_all<I>(&mut self, kits: I)
    where
        I: IntoIterator<Item = Box<dyn ChipWitnessKit>>,
    {
        for kit in kits {
            self.register(kit);
        }
    }

    /// Iterate the kits in drive order.
    pub fn iter(&self) -> impl Iterator<Item = &dyn ChipWitnessKit> {
        self.kits.iter().map(std::convert::AsRef::as_ref)
    }

    /// `true` when no kits are registered.
    pub fn is_empty(&self) -> bool {
        self.kits.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tabula_stark::chips::ChipId;
    use tabula_stark::trace::WitnessStore;
    use tabula_stark::witness_kit::{KitError, KitFinalizeContext};

    const CHIP_A: ChipId = ChipId(9001);
    const CHIP_B: ChipId = ChipId(9002);

    struct StubKit {
        id: ChipId,
        label: &'static str,
    }

    impl ChipWitnessKit for StubKit {
        fn chip_id(&self) -> ChipId {
            self.id
        }
        fn witness_store_label(&self) -> &'static str {
            self.label
        }
        fn finalize(
            &self,
            _ctx: &mut KitFinalizeContext<'_>,
            _store: &mut WitnessStore,
        ) -> Result<(), KitError> {
            Ok(())
        }
    }

    #[test]
    #[should_panic(expected = "duplicate ChipWitnessKit registration")]
    fn duplicate_chip_id_panics() {
        let mut registry = ChipKitRegistry::new();
        registry.register(Box::new(StubKit {
            id: CHIP_A,
            label: "alpha",
        }));
        registry.register(Box::new(StubKit {
            id: CHIP_A,
            label: "beta",
        }));
    }

    #[test]
    #[should_panic(expected = "duplicate ChipWitnessKit witness-store label")]
    fn duplicate_label_panics() {
        let mut registry = ChipKitRegistry::new();
        registry.register(Box::new(StubKit {
            id: CHIP_A,
            label: "shared",
        }));
        registry.register(Box::new(StubKit {
            id: CHIP_B,
            label: "shared",
        }));
    }
}
