use std::sync::Arc;

use tabula_core::RootProfileId;
use tabula_machine::{MachineBuilder, RootProof, SmtRootProof, TabulaStarkConfig};

struct SharedRootProof(Arc<dyn RootProof>);

impl RootProof for SharedRootProof {
    fn proof_family_id(&self) -> tabula_core::RootProofFamilyId {
        self.0.proof_family_id()
    }

    fn supported_root_binding_families(&self) -> &'static [RootProfileId] {
        self.0.supported_root_binding_families()
    }

    fn airs(&self) -> Vec<Box<dyn tabula_machine::backend::AnyRap>> {
        self.0.airs()
    }

    fn dyn_chips(&self) -> Vec<Box<dyn tabula_stark::trace::DynChip>> {
        self.0.dyn_chips()
    }

    fn buses(&self) -> Vec<tabula_stark::air::interaction::BusId> {
        self.0.buses()
    }
}

/// Machine-side proving and verification configuration shared by runtime and verifier builders.
#[derive(Clone)]
pub struct MachineConfig {
    config: TabulaStarkConfig,
    root_proof: Arc<dyn RootProof>,
}

impl MachineConfig {
    /// Build the standard machine configuration.
    pub fn standard() -> Self {
        Self {
            config: tabula_machine::default_config(),
            root_proof: Arc::new(SmtRootProof),
        }
    }

    /// Override the root proof backend.
    pub fn with_root_proof(mut self, root: impl RootProof + 'static) -> Self {
        self.root_proof = Arc::new(root);
        self
    }

    /// Override the STARK configuration.
    pub fn with_config(mut self, config: TabulaStarkConfig) -> Self {
        self.config = config;
        self
    }

    pub(crate) fn supported_root_binding_families(&self) -> &[RootProfileId] {
        self.root_proof.supported_root_binding_families()
    }

    pub(crate) fn build_machine_builder(&self) -> MachineBuilder {
        MachineBuilder::new()
            .with_config(self.config.clone())
            .with_root_proof(SharedRootProof(Arc::clone(&self.root_proof)))
    }
}

impl Default for MachineConfig {
    fn default() -> Self {
        Self::standard()
    }
}
