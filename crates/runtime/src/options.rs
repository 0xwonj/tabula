//! Handle-construction knob surface (§5.1).
//!
//! `PreparedOptions` bundles the host environment, machine STARK config,
//! and root-backend choice that every prepared handle needs. The free
//! functions [`crate::prepare_prover`] and [`crate::prepare_verifier`]
//! take `&PreparedOptions`; this replaces the per-knob `with_*` methods
//! on the legacy builders without changing the defaults.

#[cfg(not(feature = "prove"))]
use std::sync::Arc;

use tabula_machine::TabulaStarkConfig;

use crate::error::SetupError;
use crate::host::HostEnvironment;

/// Bundle of handle-construction knobs applied at prepare time.
#[non_exhaustive]
#[derive(Clone)]
pub struct PreparedOptions {
    host_environment: HostEnvironment,
    machine_stark_config: TabulaStarkConfig,
    root_backend: RootBackend,
}

impl PreparedOptions {
    /// Build the standard option bundle (standard host environment,
    /// default machine config, standard root backend).
    pub fn try_standard() -> Result<Self, SetupError> {
        Ok(Self {
            host_environment: HostEnvironment::standard()?,
            machine_stark_config: tabula_machine::default_config(),
            root_backend: RootBackend::standard(),
        })
    }

    /// Replace the host-owned runtime registries and scheme factories.
    pub fn with_host_environment(mut self, host_environment: HostEnvironment) -> Self {
        self.host_environment = host_environment;
        self
    }

    /// Override the machine STARK configuration.
    pub fn with_machine_stark_config(mut self, machine_stark_config: TabulaStarkConfig) -> Self {
        self.machine_stark_config = machine_stark_config;
        self
    }

    /// Override the root-backend selection.
    pub fn with_root_backend(mut self, root_backend: RootBackend) -> Self {
        self.root_backend = root_backend;
        self
    }

    /// Borrow the installed host environment.
    pub fn host_environment(&self) -> &HostEnvironment {
        &self.host_environment
    }

    /// Borrow the configured machine STARK configuration.
    pub fn machine_stark_config(&self) -> &TabulaStarkConfig {
        &self.machine_stark_config
    }

    /// Borrow the configured root backend.
    pub fn root_backend(&self) -> &RootBackend {
        &self.root_backend
    }
}

/// Root-backend selection used by prepared handles.
///
/// On `prove`, this wraps a full [`tabula_ext::root::RootBackendBundle`]
/// (witness preparer + proof backend). On verify-only builds, it wraps
/// a shared proof-only backend.
#[cfg(feature = "prove")]
#[non_exhaustive]
#[derive(Clone)]
pub struct RootBackend(pub(crate) tabula_ext::root::RootBackendBundle);

/// Root-backend selection used by prepared handles (verify-only build).
#[cfg(all(feature = "verify", not(feature = "prove")))]
#[non_exhaustive]
#[derive(Clone)]
pub struct RootBackend(pub(crate) Arc<dyn tabula_ext::root::RootProofBackend>);

impl RootBackend {
    /// Standard root backend (SMT on verify-only, bundled SMT on prove).
    #[cfg(feature = "prove")]
    pub fn standard() -> Self {
        Self(tabula_ext::root::RootBackendBundle::standard())
    }

    /// Build from an existing prove-side bundle.
    #[cfg(feature = "prove")]
    pub fn from_bundle(bundle: tabula_ext::root::RootBackendBundle) -> Self {
        Self(bundle)
    }

    /// Standard root backend (SMT).
    #[cfg(all(feature = "verify", not(feature = "prove")))]
    pub fn standard() -> Self {
        Self(Arc::new(tabula_ext::root::SmtRootProofBackend))
    }

    /// Build from a shared verify-side proof backend.
    #[cfg(all(feature = "verify", not(feature = "prove")))]
    pub fn from_proof_backend(backend: Arc<dyn tabula_ext::root::RootProofBackend>) -> Self {
        Self(backend)
    }
}
