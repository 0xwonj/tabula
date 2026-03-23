//! Fluent builder API for constructing [`TabulaMachine`] instances.
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
use crate::setup::build::{column_tier_setup, execution_tier_setup, root_tier_setup};
use crate::setup::execution::execution_dyn_chips;
use crate::setup::keys::{TabulaProvingKey, TabulaVerifyingKey};
use crate::setup::registry::{ChipRegistry, SetupError};
use crate::setup::root::{RootProof, SmtRootProof};
use crate::setup::types::{MachineSetup, ProofSetups, TierSetup};

/// Fluent builder for [`TabulaMachine`] construction.
pub struct MachineBuilder {
    columns: Vec<Arc<dyn ProofColumn>>,
    config: TabulaStarkConfig,
    root_proof: Box<dyn RootProof>,
    extensions: Vec<Box<dyn ExecutionTierExtension>>,
}

impl MachineBuilder {
    /// Create a new builder with default settings.
    pub fn new() -> Self {
        Self {
            columns: Vec::new(),
            config: default_config(),
            root_proof: Box::new(SmtRootProof),
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

    /// Override the root proof scheme.
    pub fn with_root_proof(mut self, root: impl RootProof + 'static) -> Self {
        self.root_proof = Box::new(root);
        self
    }

    /// Register a backend execution-tier extension.
    pub fn with_backend_execution_extension(
        mut self,
        ext: impl ExecutionTierExtension + 'static,
    ) -> Self {
        self.extensions.push(Box::new(ext));
        self
    }

    /// Register a boxed backend execution-tier extension.
    pub fn with_backend_execution_extension_boxed(
        mut self,
        ext: Box<dyn ExecutionTierExtension>,
    ) -> Self {
        self.extensions.push(ext);
        self
    }

    /// Build the machine from execution/root setup and column instances.
    pub fn build(self) -> Result<crate::TabulaMachine, SetupError> {
        let setup = self.build_setup()?;
        Ok(crate::TabulaMachine::from_setup(setup))
    }

    /// Build the immutable backend setup without wrapping it in [`crate::TabulaMachine`].
    pub fn build_setup(self) -> Result<MachineSetup, SetupError> {
        let setups = self.create_setups()?;
        Ok(MachineSetup::new(self.config, setups))
    }

    fn create_setups(&self) -> Result<ProofSetups, SetupError> {
        let execution = self.build_execution_tier()?;

        let columns = self
            .columns
            .iter()
            .map(|column| {
                let setup = column_tier_setup(column.as_ref())?;
                Ok(((column.table_id(), column.col_id()), setup))
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
        if self.extensions.is_empty() {
            return execution_tier_setup();
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
        registry.validate()?;

        let mut dyn_chips: Vec<Box<dyn DynChip>> = execution_dyn_chips();
        for ext in &self.extensions {
            dyn_chips.extend(ext.dyn_chips());
        }
        dyn_chips.push(Box::new(RangeCheckChip));

        let mut bus_consumers: Vec<Box<dyn BusConsumer>> = vec![Box::new(RangeCheckChip)];
        for ext in &self.extensions {
            bus_consumers.extend(ext.bus_consumers());
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
