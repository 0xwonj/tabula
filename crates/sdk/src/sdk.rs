use std::collections::BTreeMap;
use std::sync::Arc;
#[cfg(any(feature = "execute", feature = "verify"))]
use std::sync::Mutex;

#[cfg(feature = "compile")]
use tabula_compiler::compile_and_register_program_source;
use tabula_compiler::{CompiledProgram, register_compiled_program};

use crate::builder::SdkBuilder;
use crate::environment::Environment;
use crate::error::SdkError;
use crate::program::{Artifact, Program};

/// Shared process-level SDK context.
#[derive(Clone)]
pub struct Sdk {
    pub(crate) inner: Arc<SdkInner>,
}

pub(crate) struct SdkInner {
    pub(crate) environment: Environment,
    #[cfg(feature = "execute")]
    pub(crate) runtime_cache: Mutex<BTreeMap<String, Arc<tabula_runtime::TabulaRuntime>>>,
    #[cfg(feature = "verify")]
    pub(crate) verifier_cache: Mutex<BTreeMap<String, Arc<tabula_runtime::Verifier>>>,
}

impl Sdk {
    /// Build a standard SDK with built-in compiler catalogs and host environment.
    pub fn standard() -> Self {
        Self::builder()
            .build()
            .expect("built-in SDK environment must remain installable")
    }

    /// Start a customized SDK builder.
    pub fn builder() -> SdkBuilder {
        SdkBuilder::new()
    }

    /// Build one SDK from an already prepared environment.
    pub fn from_environment(environment: Environment) -> Self {
        Self {
            inner: Arc::new(SdkInner {
                environment,
                #[cfg(feature = "execute")]
                runtime_cache: Mutex::new(BTreeMap::new()),
                #[cfg(feature = "verify")]
                verifier_cache: Mutex::new(BTreeMap::new()),
            }),
        }
    }

    /// Borrow the immutable application environment bound to this SDK.
    pub fn environment(&self) -> &Environment {
        &self.inner.environment
    }

    /// Compile rewritten source into a sealed artifact.
    #[cfg(feature = "compile")]
    pub fn compile(&self, source: &str) -> Result<Artifact, SdkError> {
        let registered = compile_and_register_program_source(
            source,
            &self.inner.environment.inner.compiler_catalogs,
        )?;
        Artifact::from_registered(registered)
    }

    /// Load one serialized artifact payload from bytes.
    pub fn load_artifact(&self, bytes: &[u8]) -> Result<Artifact, SdkError> {
        serde_json::from_slice(bytes).map_err(|error| SdkError::ArtifactDecode {
            detail: error.to_string(),
        })
    }

    /// Open one artifact into a reusable semantic program handle.
    pub fn open(&self, artifact: Artifact) -> Result<Program, SdkError> {
        Ok(Program::new(self.clone(), artifact))
    }

    pub(crate) fn register_compiled(
        &self,
        compiled: CompiledProgram,
    ) -> Result<Artifact, SdkError> {
        let registered =
            register_compiled_program(compiled, &self.inner.environment.inner.compiler_catalogs)?;
        Artifact::from_registered(registered)
    }

    #[cfg(feature = "execute")]
    pub(crate) fn prepare_runtime(
        &self,
        artifact: &Artifact,
    ) -> Result<Arc<tabula_runtime::TabulaRuntime>, SdkError> {
        let key = self.cache_key("runner", artifact);
        let mut cache = self
            .inner
            .runtime_cache
            .lock()
            .expect("sdk runtime cache mutex poisoned");
        if let Some(runtime) = cache.get(&key) {
            return Ok(Arc::clone(runtime));
        }

        let built = Arc::new(self.build_runtime(artifact)?);
        cache.insert(key, Arc::clone(&built));
        Ok(built)
    }

    #[cfg(feature = "verify")]
    pub(crate) fn prepare_verifier(
        &self,
        artifact: &Artifact,
    ) -> Result<Arc<tabula_runtime::Verifier>, SdkError> {
        let key = self.cache_key("verifier", artifact);
        let mut cache = self
            .inner
            .verifier_cache
            .lock()
            .expect("sdk verifier cache mutex poisoned");
        if let Some(verifier) = cache.get(&key) {
            return Ok(Arc::clone(verifier));
        }

        let built = Arc::new(self.build_verifier(artifact)?);
        cache.insert(key, Arc::clone(&built));
        Ok(built)
    }

    #[cfg(feature = "execute")]
    fn build_runtime(
        &self,
        artifact: &Artifact,
    ) -> Result<tabula_runtime::TabulaRuntime, SdkError> {
        let builder = tabula_runtime::TabulaRuntime::builder(artifact.registered().clone())
            .with_host_environment(self.inner.environment.inner.host_environment.clone())
            .with_machine_stark_config(self.inner.environment.inner.machine_stark_config.clone());
        #[cfg(feature = "prove")]
        let builder = builder
            .with_root_backend_bundle(self.inner.environment.inner.root_backend_bundle.clone());
        builder.build().map_err(SdkError::from)
    }

    #[cfg(feature = "verify")]
    fn build_verifier(&self, artifact: &Artifact) -> Result<tabula_runtime::Verifier, SdkError> {
        let builder = tabula_runtime::Verifier::builder(artifact.registered().clone())
            .with_host_environment(self.inner.environment.inner.host_environment.clone())
            .with_machine_stark_config(self.inner.environment.inner.machine_stark_config.clone());
        #[cfg(feature = "prove")]
        let builder = builder
            .with_root_backend_bundle(self.inner.environment.inner.root_backend_bundle.clone());
        builder.build().map_err(SdkError::from)
    }

    fn cache_key(&self, mode: &str, artifact: &Artifact) -> String {
        format!(
            "{}:{}:{}",
            self.inner.environment.inner.fingerprint,
            artifact.digest(),
            mode
        )
    }
}

impl std::fmt::Debug for Sdk {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sdk")
            .field("environment", &self.inner.environment)
            .finish_non_exhaustive()
    }
}
