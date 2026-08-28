//! Fluent builder API for constructing `TabulaMachine` instances.
//!
//! The machine consumes fully built proof columns plus execution/root extensions.
//! Scheme registry ownership and semantic capability validation live in the
//! runtime layer; the backend only sees already built proof inputs.

use std::sync::Arc;

use tabula_chips::range_check::RangeCheckChip;
use tabula_stark::trace::DynChip;
use tabula_stark::trace::column_commitment::BusConsumer;

use crate::backend::extension::ExecutionTierExtension;
use crate::backend::{AnyRap, ProofColumn};
use crate::config::{TabulaStarkConfig, default_config};
use crate::setup::execution::execution_dyn_chips;
use crate::setup::recipes::{
    TierRecipe, column_tier_topology, execution_tier_topology, finalize_tier_topology,
    root_tier_topology,
};
use crate::setup::registry::{ChipRegistry, SetupError};
use crate::setup::root::{RootProofBackend, SmtRootProofBackend};
use crate::setup::topology::{MachineTopology, ProofTopology, TierTopology};

/// Fluent builder for `TabulaMachine` construction.
pub struct MachineBuilder {
    columns: Vec<Arc<dyn ProofColumn>>,
    config: TabulaStarkConfig,
    root_proof_backend: Arc<dyn RootProofBackend>,
    extensions: Vec<Box<dyn ExecutionTierExtension>>,
}

impl MachineBuilder {
    /// Create a new builder with default settings.
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
            config: default_config(),
            root_proof_backend: Arc::new(SmtRootProofBackend),
            extensions: Vec::new(),
        }
    }

    /// Set the proof columns used to build column proof tiers.
    pub fn with_columns(mut self, columns: impl IntoIterator<Item = Arc<dyn ProofColumn>>) -> Self {
        self.columns = columns.into_iter().collect();
        self
    }

    /// Override the STARK configuration.
    pub fn with_config(mut self, config: TabulaStarkConfig) -> Self {
        self.config = config;
        self
    }

    /// Override the proof-side root backend.
    pub fn with_root_proof_backend(mut self, root: impl RootProofBackend + 'static) -> Self {
        self.root_proof_backend = Arc::new(root);
        self
    }

    /// Override the proof-side root backend using a shared backend object.
    pub fn with_root_proof_backend_arc(mut self, root: Arc<dyn RootProofBackend>) -> Self {
        self.root_proof_backend = root;
        self
    }

    /// Register a machine-only backend execution-tier extension.
    pub fn with_backend_execution_extension(
        mut self,
        ext: impl ExecutionTierExtension + 'static,
    ) -> Self {
        self.extensions.push(Box::new(ext));
        self
    }

    /// Register a boxed machine-only backend execution-tier extension.
    pub fn with_backend_execution_extension_boxed(
        mut self,
        ext: Box<dyn ExecutionTierExtension>,
    ) -> Self {
        self.extensions.push(ext);
        self
    }

    /// Build the machine from execution/root topology and column instances.
    pub fn build(self) -> Result<crate::TabulaMachine, SetupError> {
        let topology = self.build_topology()?;
        Ok(crate::machine::TabulaMachine::from_topology(topology))
    }

    fn build_topology(self) -> Result<MachineTopology, SetupError> {
        let proof_topology = self.create_proof_topology()?;
        Ok(MachineTopology::new(self.config, proof_topology))
    }

    fn create_proof_topology(&self) -> Result<ProofTopology, SetupError> {
        let execution = self.build_execution_tier_topology()?;

        let columns = self
            .columns
            .iter()
            .map(|column| {
                let topology = column_tier_topology(column.as_ref())?;
                Ok(((column.table_id(), column.col_id()), topology))
            })
            .collect::<Result<Vec<_>, SetupError>>()?;

        let root = root_tier_topology(self.root_proof_backend.as_ref())?;

        Ok(ProofTopology {
            execution,
            columns,
            root,
        })
    }

    fn build_execution_tier_topology(&self) -> Result<TierTopology, SetupError> {
        if self.extensions.is_empty() {
            return execution_tier_topology();
        }

        let mut registry = ChipRegistry::new();
        registry.register_execution();

        for ext in &self.extensions {
            let airs = ext.airs();
            let dyn_chip_list = ext.dyn_chips();
            validate_chip_id_consistency(&airs, &dyn_chip_list, ext.name())?;
            registry.register_boxed(airs);
        }

        registry.register(RangeCheckChip);

        let mut dyn_chips: Vec<Box<dyn DynChip>> = execution_dyn_chips();
        for ext in &self.extensions {
            dyn_chips.extend(ext.dyn_chips());
        }
        dyn_chips.push(Box::new(RangeCheckChip));

        let mut bus_consumers: Vec<Box<dyn BusConsumer>> = vec![Box::new(RangeCheckChip)];
        for ext in &self.extensions {
            bus_consumers.extend(ext.bus_consumers());
        }

        finalize_tier_topology(TierRecipe {
            registry,
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

fn validate_chip_id_consistency(
    airs: &[Box<dyn AnyRap>],
    dyn_chips: &[Box<dyn DynChip>],
    ext_name: &str,
) -> Result<(), SetupError> {
    use std::collections::BTreeSet;

    let air_ids: BTreeSet<_> = airs.iter().map(|air| air.chip_id()).collect();
    let dyn_ids: BTreeSet<_> = dyn_chips.iter().map(|chip| chip.chip_id()).collect();

    if air_ids != dyn_ids {
        return Err(SetupError::SetupFailed(format!(
            "extension '{ext_name}' returned mismatched AIR/DynChip chip IDs: airs={air_ids:?}, dyn={dyn_ids:?}",
        )));
    }

    Ok(())
}
