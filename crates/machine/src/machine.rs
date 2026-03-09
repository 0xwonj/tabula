//! High-level machine interface for STARK proving and verification.
//!
//! [`TabulaMachine`] is the single source of truth for the entire proof pipeline:
//! trace building, proving, and verification. Use [`MachineBuilder`] to construct.
//!
//! # Layered composition
//!
//! The proof system is organized into layers:
//! - **Layer 0 (Core)**: Execution, memory model, root proof, bus consumers
//! - **Layer 1 (Commitment)**: Pluggable SSMC/SMT/custom schemes
//!
//! ```ignore
//! // Default: 8 core + 1 commitment = 9 chips
//! let machine = TabulaMachine::builder()
//!     .with_core_chips()
//!     .with_default_commitments()
//!     .build()?;
//!
//! // Custom commitment scheme
//! let machine = TabulaMachine::builder()
//!     .with_core_chips()
//!     .with_chip(MyAccumulatorChip)
//!     .build()?;
//! ```

use tabula_core::error::TabulaError;
use tabula_stark::air::interaction::BusId;
use tabula_stark::air::statement::PublicStatement;
use tabula_stark::trace::{BusConsumer, DynChip, WitnessStore};
use tabula_witness::trace::TraceMap;

use std::collections::BTreeSet;
use std::fmt;

use crate::composition::{
    CommitmentScheme, GlobalSortedMemory, MemoryModel, RootProof, SmtRootProof, SsmcScheme,
    execution_dyn_chips,
};
use crate::config::{TabulaStarkConfig, default_config};
use crate::keys::{TabulaProvingKey, TabulaVerifyingKey};
use crate::proof::{ProveError, TabulaProof, VerificationError};
use crate::registry::{ChipRegistry, SetupError};
use crate::AnyRap;

/// A configured STARK machine ready for trace building, proving, and verification.
///
/// Owns the complete chip configuration — both AIR constraints (for proving) and
/// trace contributors (for witness-to-trace conversion). This makes it the single
/// source of truth: adding a chip via the builder automatically registers it for
/// both trace building and proving.
///
/// `Debug` is intentionally manual because `TabulaStarkConfig` does not
/// implement `Debug`.
pub struct TabulaMachine {
    config: TabulaStarkConfig,
    registry: ChipRegistry,
    dyn_chips: Vec<Box<dyn DynChip>>,
    bus_consumers: Vec<Box<dyn BusConsumer>>,
    buses: Vec<BusId>,
    proving_key: TabulaProvingKey,
    verifying_key: TabulaVerifyingKey,
}

impl fmt::Debug for TabulaMachine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TabulaMachine")
            .field("chip_ids", &self.registry.chip_ids())
            .finish_non_exhaustive()
    }
}

impl TabulaMachine {
    /// Start building a new machine.
    pub fn builder() -> MachineBuilder {
        MachineBuilder {
            registry: ChipRegistry::new(),
            dyn_chips: Vec::new(),
            bus_consumers: Vec::new(),
            buses: Vec::new(),
            config: None,
        }
    }

    /// Build chip traces from a populated [`WitnessStore`].
    ///
    /// Uses the machine's own DynChip and BusConsumer configuration — the same
    /// chips registered via the builder are used for trace construction.
    ///
    /// Typical flow:
    /// ```ignore
    /// let store = TraceBuilder::new(&witness)
    ///     .prepare_witness_store(&program, &batch, &result, &schemas, &statics, hasher)?;
    /// let traces = machine.build_traces(store)?;
    /// let proof = machine.prove(&traces, statement)?;
    /// ```
    pub fn build_traces(&self, store: WitnessStore) -> Result<TraceMap, TabulaError> {
        tabula_witness::trace::build_all_traces(&self.dyn_chips, &self.bus_consumers, store)
    }

    /// Validate chip traces with debug constraints and bus balance checks.
    ///
    /// For each chip, evaluates AIR constraints and checks LogUp bus balance.
    /// Uses the machine's own chip configuration for validation.
    pub fn debug_validate(&self, traces: &TraceMap) -> Result<(), TabulaError> {
        tabula_witness::trace::debug_validate_trace_map(&self.dyn_chips, &self.buses, traces)
    }

    /// Generate a STARK proof from a [`TraceMap`] and public statement.
    pub fn prove(
        &self,
        traces: &TraceMap,
        statement: PublicStatement,
    ) -> Result<TabulaProof, ProveError> {
        crate::prove::prove_with_key(
            &self.config,
            &self.registry,
            &self.proving_key,
            traces,
            statement,
        )
    }

    /// Verify a STARK proof.
    pub fn verify(&self, proof: &TabulaProof) -> Result<(), VerificationError> {
        crate::verify::verify_with_key(&self.config, &self.registry, &self.verifying_key, proof)
    }

    /// The chip registry (AIR implementations for proving/verification).
    pub fn registry(&self) -> &ChipRegistry {
        &self.registry
    }

    /// The STARK configuration.
    pub fn config(&self) -> &TabulaStarkConfig {
        &self.config
    }

    /// The proving key (cached keygen info).
    pub fn proving_key(&self) -> &TabulaProvingKey {
        &self.proving_key
    }

    /// The verifying key (minimal verification metadata).
    pub fn verifying_key(&self) -> &TabulaVerifyingKey {
        &self.verifying_key
    }
}

/// Builder for [`TabulaMachine`].
///
/// Every `with_*` method registers chips for both proving (AIR) and trace building
/// (DynChip) simultaneously. This ensures the machine is always internally consistent.
///
/// ```ignore
/// // Full default (9 chips: 8 core + 1 commitment)
/// let machine = TabulaMachine::builder()
///     .with_core_chips()
///     .with_default_commitments()
///     .build()?;
///
/// // Custom memory model (advanced)
/// let machine = TabulaMachine::builder()
///     .with_execution()
///     .with_memory_model(&PermutationMemory)
///     .with_root_proof(&SmtRootProof)
///     .with_bus_consumers()
///     .with_default_commitments()
///     .build()?;
/// ```
pub struct MachineBuilder {
    registry: ChipRegistry,
    dyn_chips: Vec<Box<dyn DynChip>>,
    bus_consumers: Vec<Box<dyn BusConsumer>>,
    buses: Vec<BusId>,
    config: Option<TabulaStarkConfig>,
}

impl MachineBuilder {
    /// Register Layer 0 core chips (8 chips).
    ///
    /// Registers execution, memory model, root proof, and bus consumer chips
    /// for both proving and trace building.
    ///
    /// Typical usage: `with_core_chips().with_default_commitments()` for the full
    /// 9-chip default set.
    pub fn with_core_chips(self) -> Self {
        self.with_execution()
            .with_memory_model(&GlobalSortedMemory)
            .with_root_proof(&SmtRootProof)
            .with_bus_consumers()
    }

    /// Register default commitment-layer chips (SSMC + SMT).
    ///
    /// Adds SSMC (StateColumnChip) and SMT (no extra chip) commitment schemes.
    /// SMT columns need no additional chip — root verification is in Layer 0.
    ///
    /// Equivalent to `.with_commitment(&SsmcScheme)`.
    pub fn with_default_commitments(self) -> Self {
        self.with_commitment(&SsmcScheme)
    }

    /// Register a commitment scheme's chips for proving and trace building.
    ///
    /// Each scheme provides AIR chips (for proving/verification) and DynChips
    /// (for trace building). Multiple schemes can be registered — their chips
    /// are additive.
    ///
    /// ```ignore
    /// // Custom commitment scheme
    /// let machine = TabulaMachine::builder()
    ///     .with_core_chips()
    ///     .with_commitment(&SsmcScheme)
    ///     .with_commitment(&MyAccumulatorScheme)
    ///     .build()?;
    /// ```
    pub fn with_commitment(mut self, scheme: &dyn CommitmentScheme) -> Self {
        self.registry.register_boxed(scheme.airs());
        self.dyn_chips.extend(scheme.dyn_chips());
        self.buses.extend(scheme.buses());
        self
    }

    /// Register execution-layer chips (ExecutionChip, StaticTableChip).
    pub fn with_execution(mut self) -> Self {
        self.registry.register_execution();
        self.dyn_chips.extend(execution_dyn_chips());
        self
    }

    /// Register memory model chips.
    pub fn with_memory_model(mut self, model: &dyn MemoryModel) -> Self {
        self.registry.register_memory_model(model);
        self.dyn_chips.extend(model.dyn_chips());
        self.buses.extend(model.buses());
        self
    }

    /// Register root proof chips.
    pub fn with_root_proof(mut self, proof: &dyn RootProof) -> Self {
        self.registry.register_root_proof(proof);
        self.dyn_chips.extend(proof.dyn_chips());
        self.buses.extend(proof.buses());
        self
    }

    /// Register bus consumer chips (PoseidonChip, RangeCheckChip).
    pub fn with_bus_consumers(mut self) -> Self {
        use tabula_chips::poseidon::PoseidonChip;
        use tabula_chips::range_check::RangeCheckChip;

        self.registry.register_bus_consumers();
        self.dyn_chips.push(Box::new(PoseidonChip));
        self.dyn_chips.push(Box::new(RangeCheckChip));
        self.bus_consumers.push(Box::new(PoseidonChip));
        self.bus_consumers.push(Box::new(RangeCheckChip));
        self
    }

    /// Register a single custom chip (AIR only).
    ///
    /// The chip is registered for proving/verification but NOT for trace building.
    /// Use this when the chip's trace is built externally and included in the
    /// [`WitnessStore`] before calling [`TabulaMachine::build_traces()`].
    pub fn with_chip(mut self, chip: impl AnyRap + 'static) -> Self {
        self.registry.register(chip);
        self
    }

    /// Register a custom bus consumer for both trace building and bus collection.
    ///
    /// The consumer is added to both `dyn_chips` (for phase-ordered trace building)
    /// and `bus_consumers` (for interaction collection between Phase 1 and Dependent).
    /// It is also registered in the AIR registry for proving/verification.
    ///
    /// Use this for chips like custom accumulators that consume bus interactions
    /// and need their own trace + AIR constraints.
    pub fn with_bus_consumer(mut self, consumer: impl BusConsumer + DynChip + AnyRap + Clone + 'static) -> Self {
        self.registry.register(consumer.clone());
        self.dyn_chips.push(Box::new(consumer.clone()));
        self.bus_consumers.push(Box::new(consumer));
        self
    }

    /// Register custom bus IDs for validation.
    ///
    /// Core buses are always included. Use this to add app-defined buses.
    pub fn with_buses(mut self, buses: impl IntoIterator<Item = BusId>) -> Self {
        self.buses.extend(buses);
        self
    }

    /// Set a custom STARK configuration. Defaults to [`default_config()`] if not called.
    pub fn with_config(mut self, config: TabulaStarkConfig) -> Self {
        self.config = Some(config);
        self
    }

    /// Build the machine, validating the registry and computing keys.
    ///
    /// Runs keygen once to produce the [`TabulaProvingKey`] and derives
    /// the [`TabulaVerifyingKey`] from it.
    pub fn build(self) -> Result<TabulaMachine, SetupError> {
        self.registry.validate()?;

        // Every DynChip must have a corresponding AIR in the registry.
        // The reverse is allowed: `with_chip()` registers AIR-only chips
        // whose traces are built externally.
        let registry_ids: BTreeSet<_> = self.registry.chip_ids().into_iter().collect();
        let mut seen_dyn_chips = BTreeSet::new();
        for dyn_chip in &self.dyn_chips {
            let id = dyn_chip.chip_id();
            if !registry_ids.contains(&id) {
                return Err(SetupError::DynChipWithoutAir(id));
            }
            if !seen_dyn_chips.insert(id) {
                return Err(SetupError::DuplicateChipId(id));
            }
        }

        let proving_key = TabulaProvingKey::from_registry(&self.registry);
        let verifying_key = TabulaVerifyingKey::from_proving_key(&proving_key);

        // Always include core buses; app buses are additive.
        let mut buses = tabula_stark::air::interaction::core_buses::ALL.to_vec();
        buses.extend(self.buses);

        Ok(TabulaMachine {
            config: self.config.unwrap_or_else(default_config),
            registry: self.registry,
            dyn_chips: self.dyn_chips,
            bus_consumers: self.bus_consumers,
            buses,
            proving_key,
            verifying_key,
        })
    }
}
