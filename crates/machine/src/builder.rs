//! Fluent builder API for constructing [`TabulaMachine`] instances.
//!
//! The builder composes core chips, extensions, and configuration into
//! a configured machine. This is the primary entry point for applications
//! that customize Tabula's proof pipeline.
//!
//! ```ignore
//! use tabula_machine::{MachineBuilder, ColumnSetupConfig};
//!
//! let machine = TabulaMachine::builder()
//!     .with_columns(col_configs)
//!     .with_extension(MyExtension)
//!     .build()?;
//! ```

use std::collections::{BTreeMap, BTreeSet};

use tabula_chips::poseidon::PoseidonChip;
use tabula_chips::range_check::RangeCheckChip;
use tabula_commitment::scheme_tags;
use tabula_core::{ColId, TableId};
use tabula_stark::chips::DEFAULT_VALUE_WIDTH;
use tabula_stark::trace::DynChip;
use tabula_stark::trace::column_commitment::BusConsumer;

use crate::AnyRap;
use crate::column_scheme::{ColumnScheme, SsmcScheme};
use crate::composition::{RootProof, SmtRootProof, execution_dyn_chips};
use crate::config::{TabulaStarkConfig, default_config};
use crate::extension::ChipExtension;
use crate::keys::{TabulaProvingKey, TabulaVerifyingKey};
use crate::property::PropertyOpening;
use crate::registry::{ChipRegistry, SetupError};
use crate::setup::{
    ColumnSetupConfig, ProofSetups, TierSetup, column_tier_setup_with_scheme, execution_tier_setup,
    root_tier_setup,
};

/// Fluent builder for [`TabulaMachine`] construction.
///
/// Collects configuration, column definitions, extensions, and commitment
/// schemes, then assembles per-tier proof setups on [`build()`](Self::build).
///
/// # Defaults
///
/// - **STARK config**: [`default_config()`]
/// - **Root proof**: [`SmtRootProof`] (two-level SMT)
/// - **Column scheme**: [`SsmcScheme`] for `scheme_tags::SSMC`
/// - **Core chips**: Always included (ExecutionChip, StaticTableChip,
///   shard chips, PoseidonChip, RangeCheckChip)
pub struct MachineBuilder {
    col_configs: Vec<ColumnSetupConfig>,
    config: TabulaStarkConfig,
    root_proof: Box<dyn RootProof>,
    extensions: Vec<Box<dyn ChipExtension>>,
    column_schemes: BTreeMap<u16, Box<dyn ColumnScheme>>,
    property_openings: Vec<Box<dyn PropertyOpening>>,
}

impl MachineBuilder {
    /// Create a new builder with default settings.
    ///
    /// Pre-registers `SsmcScheme<3>` for `scheme_tags::SSMC`.
    pub fn new() -> Self {
        let mut column_schemes: BTreeMap<u16, Box<dyn ColumnScheme>> = BTreeMap::new();
        column_schemes.insert(
            scheme_tags::SSMC,
            Box::new(SsmcScheme::<DEFAULT_VALUE_WIDTH>),
        );
        Self {
            col_configs: Vec::new(),
            config: default_config(),
            root_proof: Box::new(SmtRootProof),
            extensions: Vec::new(),
            column_schemes,
            property_openings: Vec::new(),
        }
    }

    /// Set the column configurations.
    ///
    /// Each column gets its own proof tier with shard chips determined
    /// by its `scheme_tag` and the registered [`ColumnScheme`].
    pub fn with_columns(mut self, configs: Vec<ColumnSetupConfig>) -> Self {
        self.col_configs = configs;
        self
    }

    /// Override the STARK configuration.
    ///
    /// Default: [`default_config()`] (BLAKE3 Merkle, Poseidon2 Fiat-Shamir).
    pub fn with_config(mut self, config: TabulaStarkConfig) -> Self {
        self.config = config;
        self
    }

    /// Override the root proof scheme.
    ///
    /// Default: [`SmtRootProof`] (two-level SMT with column and table path chips).
    pub fn with_root_proof(mut self, root: impl RootProof + 'static) -> Self {
        self.root_proof = Box::new(root);
        self
    }

    /// Register a chip extension for the execution tier.
    ///
    /// Extension chips are added to the execution tier's registry and trace
    /// pipeline alongside core chips. Multiple extensions can be registered;
    /// they are applied in order.
    pub fn with_extension(mut self, ext: impl ChipExtension + 'static) -> Self {
        self.extensions.push(Box::new(ext));
        self
    }

    /// Register a custom column commitment scheme.
    ///
    /// Maps a `scheme_tag` (as used in [`ColumnSetupConfig`]) to a
    /// [`ColumnScheme`] implementation. Overrides any previously registered
    /// scheme for the same tag.
    ///
    /// ```ignore
    /// use tabula_commitment::scheme_tags;
    ///
    /// TabulaMachine::builder()
    ///     .with_column_scheme(scheme_tags::SMT, SmtScheme::<3>)
    ///     .with_columns(col_configs)
    ///     .build()?;
    /// ```
    pub fn with_column_scheme(
        mut self,
        scheme_tag: u16,
        scheme: impl ColumnScheme + 'static,
    ) -> Self {
        self.column_schemes.insert(scheme_tag, Box::new(scheme));
        self
    }

    /// Register a property opening for structural queries on committed columns.
    ///
    /// Property openings enable provable queries (min, max, successor, etc.)
    /// on committed column state. Each opening declares which commitment
    /// scheme it is compatible with via [`PropertyOpening::compatible_scheme_tag()`].
    ///
    /// If the opening provides a column verifier (via
    /// [`PropertyOpening::column_verifier()`]), its chips are automatically
    /// registered. (Currently in execution tier; will move to column tier
    /// when PROPERTY_READ cross-tier bus is implemented in Goal 7 P5.)
    ///
    /// ```ignore
    /// TabulaMachine::builder()
    ///     .with_property_opening(OrderbookMinOpening)
    ///     .build()?;
    /// ```
    pub fn with_property_opening(mut self, opening: impl PropertyOpening + 'static) -> Self {
        self.property_openings.push(Box::new(opening));
        self
    }

    /// Build the machine, creating per-tier proof setups.
    ///
    /// Validates:
    /// - Duplicate ChipId detection across all registries
    /// - ChipId consistency between AIRs and DynChips for each extension
    /// - Property opening scheme_tag compatibility with registered schemes
    pub fn build(self) -> Result<crate::TabulaMachine, SetupError> {
        self.validate_property_openings()?;
        let setups = self.create_setups()?;
        Ok(crate::TabulaMachine::from_parts(
            self.config,
            setups,
            self.property_openings,
        ))
    }

    /// Validate property opening compatibility at build time.
    ///
    /// Checks:
    /// 1. Opening's scheme_tag has a registered ColumnScheme
    /// 2. Opening's supported queries ⊆ scheme's supported_property_queries
    fn validate_property_openings(&self) -> Result<(), SetupError> {
        for opening in &self.property_openings {
            let tag = opening.compatible_scheme_tag();
            let scheme = match self.column_schemes.get(&tag) {
                Some(s) => s,
                None if self.col_configs.is_empty() => continue,
                None => {
                    return Err(SetupError::SetupFailed(format!(
                        "property opening '{}' requires scheme tag {} which is not registered",
                        opening.name(),
                        tag,
                    )));
                }
            };

            // Validate: opening only claims queries the scheme can support.
            let scheme_capabilities = scheme.supported_property_queries();
            for query_kind in opening.supported_queries() {
                if !scheme_capabilities.contains(query_kind) {
                    return Err(SetupError::SetupFailed(format!(
                        "property opening '{}' claims {:?} support, but scheme '{}' \
                         (tag {}) does not support this query structurally",
                        opening.name(),
                        query_kind,
                        scheme.name(),
                        tag,
                    )));
                }
            }
        }
        Ok(())
    }

    fn create_setups(&self) -> Result<ProofSetups, SetupError> {
        let execution = self.build_execution_tier()?;

        let columns: Vec<((TableId, ColId), TierSetup)> = self
            .col_configs
            .iter()
            .map(|cfg| {
                let scheme = self.column_schemes.get(&cfg.scheme_tag).ok_or_else(|| {
                    SetupError::SetupFailed(format!(
                        "no column scheme registered for tag {}",
                        cfg.scheme_tag
                    ))
                })?;
                let setup = column_tier_setup_with_scheme(cfg, scheme.as_ref())?;
                Ok(((cfg.table_id, cfg.col_id), setup))
            })
            .collect::<Result<Vec<_>, SetupError>>()?;

        let root = root_tier_setup(self.root_proof.as_ref())?;

        Ok(ProofSetups {
            execution,
            columns,
            root,
        })
    }

    fn build_execution_tier(&self) -> Result<TierSetup, SetupError> {
        let has_extensions = !self.extensions.is_empty();
        let has_verifier_extensions = self
            .property_openings
            .iter()
            .any(|o| o.column_verifier().is_some());

        // Fast path: no extensions and no property verifier chips.
        if !has_extensions && !has_verifier_extensions {
            return execution_tier_setup();
        }

        let mut registry = ChipRegistry::new();
        registry.register_execution();

        // Register extension chips with ChipId consistency validation (H1).
        for ext in &self.extensions {
            let airs = ext.airs();
            let dyn_chip_list = ext.dyn_chips();
            validate_chip_id_consistency(&airs, &dyn_chip_list, ext.name())?;
            registry.register_boxed(airs);
        }

        // Register property opening verifier chips (H3).
        for opening in &self.property_openings {
            if let Some(verifier) = opening.column_verifier() {
                let airs = verifier.airs();
                let dyn_chip_list = verifier.dyn_chips();
                validate_chip_id_consistency(
                    &airs,
                    &dyn_chip_list,
                    &format!("{}/verifier", opening.name()),
                )?;
                registry.register_boxed(airs);
            }
        }

        registry.register_bus_consumers();
        registry.validate()?;

        let mut dyn_chips: Vec<Box<dyn DynChip>> = execution_dyn_chips();
        for ext in &self.extensions {
            dyn_chips.extend(ext.dyn_chips());
        }
        for opening in &self.property_openings {
            if let Some(verifier) = opening.column_verifier() {
                dyn_chips.extend(verifier.dyn_chips());
            }
        }
        dyn_chips.push(Box::new(PoseidonChip));
        dyn_chips.push(Box::new(RangeCheckChip));

        let mut bus_consumers: Vec<Box<dyn BusConsumer>> =
            vec![Box::new(PoseidonChip), Box::new(RangeCheckChip)];
        for ext in &self.extensions {
            bus_consumers.extend(ext.bus_consumers());
        }
        for opening in &self.property_openings {
            if let Some(verifier) = opening.column_verifier() {
                bus_consumers.extend(verifier.bus_consumers());
            }
        }

        let proving_key = TabulaProvingKey::from_registry(&registry);
        let verifying_key = TabulaVerifyingKey::from_proving_key(&proving_key);

        Ok(TierSetup {
            registry,
            proving_key,
            verifying_key,
            dyn_chips,
            bus_consumers,
        })
    }
}

impl Default for MachineBuilder {
    fn default() -> Self {
        Self::new()
    }
}

// ── Validation helpers ───────────────────────────────────────────────────────

/// Validate that AIR and DynChip lists contain the same set of ChipIds.
///
/// An AIR/DynChip mismatch would cause silent trace data loss at prove time.
/// This check catches the mismatch early during machine setup.
fn validate_chip_id_consistency(
    airs: &[Box<dyn AnyRap>],
    dyn_chips: &[Box<dyn DynChip>],
    source: &str,
) -> Result<(), SetupError> {
    let air_ids: BTreeSet<_> = airs.iter().map(|a| a.chip_id()).collect();
    let dyn_ids: BTreeSet<_> = dyn_chips.iter().map(|d| d.chip_id()).collect();

    if air_ids != dyn_ids {
        let in_air_only: Vec<_> = air_ids.difference(&dyn_ids).collect();
        let in_dyn_only: Vec<_> = dyn_ids.difference(&air_ids).collect();
        return Err(SetupError::SetupFailed(format!(
            "ChipId mismatch in '{source}': AIR-only={in_air_only:?}, DynChip-only={in_dyn_only:?}",
        )));
    }
    Ok(())
}
