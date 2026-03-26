use std::fmt;
use std::sync::Arc;

use tabula_compiler::CompilerCatalogs;
#[cfg(feature = "execute")]
use tabula_machine::TabulaStarkConfig;
use tabula_runtime::HostEnvironment;

#[cfg(feature = "prove")]
use tabula_ext::root::RootBackendBundle;

#[derive(Clone)]
pub struct Environment {
    pub(crate) inner: Arc<EnvironmentInner>,
}

pub(crate) struct EnvironmentInner {
    pub(crate) compiler_catalogs: CompilerCatalogs,
    pub(crate) host_environment: HostEnvironment,
    pub(crate) fingerprint: u64,
    #[cfg(feature = "execute")]
    pub(crate) machine_stark_config: TabulaStarkConfig,
    #[cfg(feature = "prove")]
    pub(crate) root_backend_bundle: RootBackendBundle,
}

impl Environment {
    pub(crate) fn new(inner: EnvironmentInner) -> Self {
        Self {
            inner: Arc::new(inner),
        }
    }

    pub fn fingerprint(&self) -> u64 {
        self.inner.fingerprint
    }
}

impl fmt::Debug for Environment {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Environment")
            .field("fingerprint", &self.inner.fingerprint)
            .finish_non_exhaustive()
    }
}
