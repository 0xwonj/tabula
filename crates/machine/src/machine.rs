//! High-level machine interface for STARK proving and verification.
//!
//! [`TabulaMachine`] composes a [`ChipRegistry`] with a [`TabulaStarkConfig`]
//! and exposes `prove()` / `verify()` methods. Use [`MachineBuilder`] to construct.

use tabula_stark::air::statement::PublicStatement;
use tabula_witness::trace::TraceMap;

use std::fmt;

use crate::config::{TabulaStarkConfig, default_config};
use crate::keys::{TabulaProvingKey, TabulaVerifyingKey};
use crate::proof::{ProveError, TabulaProof, VerificationError};
use crate::registry::{ChipRegistry, SetupError};
use crate::AnyRap;

/// A configured STARK machine ready for proving and verification.
///
/// Holds a validated [`ChipRegistry`], cached keys, and a [`TabulaStarkConfig`].
/// Constructed via [`TabulaMachine::builder()`].
///
/// `Debug` is intentionally manual because `TabulaStarkConfig` does not
/// implement `Debug`.
pub struct TabulaMachine {
    config: TabulaStarkConfig,
    registry: ChipRegistry,
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
            config: None,
        }
    }

    /// Generate a STARK proof from a [`TraceMap`] and public statement.
    ///
    /// Uses dynamic dispatch via [`ChipRegistry`] + [`ChipRef`](crate::ChipRef).
    /// Keygen info is cached in the [`TabulaProvingKey`] at build time.
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
    ///
    /// Uses dynamic dispatch via [`ChipRegistry`] + [`ChipRef`](crate::ChipRef).
    pub fn verify(&self, proof: &TabulaProof) -> Result<(), VerificationError> {
        crate::verify::verify_with_key(&self.config, &self.registry, &self.verifying_key, proof)
    }

    /// The chip registry.
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
/// ```ignore
/// let machine = TabulaMachine::builder()
///     .with_core_chips()
///     .build()
///     .expect("valid machine");
/// ```
pub struct MachineBuilder {
    registry: ChipRegistry,
    config: Option<TabulaStarkConfig>,
}

impl MachineBuilder {
    /// Register all 9 core Tabula chips.
    pub fn with_core_chips(mut self) -> Self {
        self.registry.register_core();
        self
    }

    /// Register a single chip.
    pub fn with_chip(mut self, chip: impl AnyRap + 'static) -> Self {
        self.registry.register(chip);
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
    /// the [`TabulaVerifyingKey`] from it. Subsequent `prove()` and `verify()`
    /// calls use the cached keys.
    pub fn build(self) -> Result<TabulaMachine, SetupError> {
        self.registry.validate()?;
        let proving_key = TabulaProvingKey::from_registry(&self.registry);
        let verifying_key = TabulaVerifyingKey::from_proving_key(&proving_key);
        Ok(TabulaMachine {
            config: self.config.unwrap_or_else(default_config),
            registry: self.registry,
            proving_key,
            verifying_key,
        })
    }
}
