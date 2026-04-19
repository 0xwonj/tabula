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
    pub(crate) verifier_cache: Mutex<BTreeMap<String, Arc<tabula_runtime::PreparedVerifier>>>,
}

impl Sdk {
    /// Build a standard SDK with built-in compiler catalogs and host environment.
    pub fn standard() -> Result<Self, SdkError> {
        Self::builder()?.build()
    }

    /// Start a customized SDK builder.
    pub fn builder() -> Result<SdkBuilder, SdkError> {
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
        let cache = self
            .inner
            .runtime_cache
            .lock()
            .map_err(|_| SdkError::Synchronization {
                detail: "sdk runtime cache mutex poisoned".to_string(),
            })?;
        if let Some(runtime) = cache.get(&key) {
            return Ok(Arc::clone(runtime));
        }
        drop(cache);

        let built = Arc::new(self.build_runtime(artifact)?);
        let mut cache = self
            .inner
            .runtime_cache
            .lock()
            .map_err(|_| SdkError::Synchronization {
                detail: "sdk runtime cache mutex poisoned".to_string(),
            })?;
        if let Some(runtime) = cache.get(&key) {
            return Ok(Arc::clone(runtime));
        }
        cache.insert(key, Arc::clone(&built));
        Ok(built)
    }

    #[cfg(feature = "verify")]
    pub(crate) fn prepare_verifier(
        &self,
        artifact: &Artifact,
    ) -> Result<Arc<tabula_runtime::PreparedVerifier>, SdkError> {
        let key = self.cache_key("verifier", artifact);
        let cache = self
            .inner
            .verifier_cache
            .lock()
            .map_err(|_| SdkError::Synchronization {
                detail: "sdk verifier cache mutex poisoned".to_string(),
            })?;
        if let Some(verifier) = cache.get(&key) {
            return Ok(Arc::clone(verifier));
        }
        drop(cache);

        let built = Arc::new(self.build_verifier(artifact)?);
        let mut cache =
            self.inner
                .verifier_cache
                .lock()
                .map_err(|_| SdkError::Synchronization {
                    detail: "sdk verifier cache mutex poisoned".to_string(),
                })?;
        if let Some(verifier) = cache.get(&key) {
            return Ok(Arc::clone(verifier));
        }
        cache.insert(key, Arc::clone(&built));
        Ok(built)
    }

    #[cfg(feature = "execute")]
    fn build_runtime(
        &self,
        artifact: &Artifact,
    ) -> Result<tabula_runtime::TabulaRuntime, SdkError> {
        let builder = tabula_runtime::TabulaRuntime::builder(artifact.registered().clone())
            .map_err(SdkError::from)?
            .with_host_environment(self.inner.environment.inner.host_environment.clone())
            .with_machine_stark_config(self.inner.environment.inner.machine_stark_config.clone());
        #[cfg(feature = "prove")]
        let builder = builder
            .with_root_backend_bundle(self.inner.environment.inner.root_backend_bundle.clone());
        builder.build().map_err(SdkError::from)
    }

    #[cfg(feature = "verify")]
    fn build_verifier(
        &self,
        artifact: &Artifact,
    ) -> Result<tabula_runtime::PreparedVerifier, SdkError> {
        let builder = tabula_runtime::PreparedVerifier::builder(artifact.registered().clone())
            .map_err(SdkError::from)?
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

#[cfg(test)]
mod tests {
    use super::*;

    use crate::environment::{Environment, EnvironmentInner};

    #[cfg(feature = "execute")]
    use tabula_runtime::{HostEnvironment, RuntimeError};

    const SIMPLE_SOURCE: &str = r#"
program RuntimeCache

state {
  table accounts(key id: u64) {
    balance: u64 @ssmc;
  }
}

tx touch(id: u64) {
  let balance = accounts[id].balance;
  assert balance >= 0;
  return;
}
"#;

    #[cfg(all(feature = "compile", feature = "execute"))]
    fn compile_simple_artifact(sdk: &Sdk) -> Artifact {
        sdk.compile(SIMPLE_SOURCE).expect("compile simple source")
    }

    #[cfg(all(feature = "compile", feature = "execute"))]
    fn sdk_with_empty_runtime_host() -> Sdk {
        let environment = Environment::new(EnvironmentInner {
            compiler_catalogs: tabula_compiler::CompilerCatalogs::standard()
                .expect("standard compiler catalogs"),
            host_environment: HostEnvironment::empty(),
            fingerprint: 7,
            machine_stark_config: tabula_machine::default_config(),
            #[cfg(feature = "prove")]
            root_backend_bundle: tabula_ext::root::RootBackendBundle::standard(),
        });
        Sdk::from_environment(environment)
    }

    #[cfg(all(feature = "compile", feature = "execute"))]
    #[test]
    fn prepare_runtime_reuses_cached_instance() {
        let sdk = Sdk::standard().expect("build standard sdk");
        let artifact = compile_simple_artifact(&sdk);

        let first = sdk.prepare_runtime(&artifact).expect("build runtime");
        let second = sdk.prepare_runtime(&artifact).expect("reuse runtime");

        assert!(Arc::ptr_eq(&first, &second));
        let cache = sdk.inner.runtime_cache.lock().expect("runtime cache");
        assert_eq!(cache.len(), 1);
    }

    #[cfg(all(feature = "compile", feature = "execute"))]
    #[test]
    fn prepare_runtime_build_failure_does_not_poison_cache() {
        let sdk = sdk_with_empty_runtime_host();
        let artifact = compile_simple_artifact(&sdk);

        let Err(first) = sdk.prepare_runtime(&artifact) else {
            panic!("runtime build must fail without host environment");
        };
        let Err(second) = sdk.prepare_runtime(&artifact) else {
            panic!("repeated runtime build failure must stay recoverable");
        };

        assert!(matches!(
            first,
            SdkError::Runtime(RuntimeError::ValidationFailed { .. })
        ));
        assert!(matches!(
            second,
            SdkError::Runtime(RuntimeError::ValidationFailed { .. })
        ));
        assert!(sdk.inner.runtime_cache.lock().is_ok());
    }

    #[cfg(all(feature = "compile", feature = "execute"))]
    #[test]
    fn poisoned_runtime_cache_returns_typed_error() {
        let sdk = Sdk::standard().expect("build standard sdk");
        let poisoned = sdk.clone();
        let join = std::thread::spawn(move || {
            let _guard = poisoned.inner.runtime_cache.lock().expect("runtime cache");
            panic!("poison runtime cache mutex");
        });
        assert!(join.join().is_err(), "poisoning thread must panic");

        let artifact = compile_simple_artifact(&sdk);
        let Err(error) = sdk.prepare_runtime(&artifact) else {
            panic!("poisoned cache must surface as typed error");
        };

        assert!(matches!(error, SdkError::Synchronization { .. }));
    }

    #[cfg(all(feature = "compile", feature = "verify", feature = "execute"))]
    #[test]
    fn prepare_verifier_reuses_cached_instance() {
        let sdk = Sdk::standard().expect("build standard sdk");
        let artifact = sdk.compile(SIMPLE_SOURCE).expect("compile simple source");

        let first = sdk.prepare_verifier(&artifact).expect("build verifier");
        let second = sdk.prepare_verifier(&artifact).expect("reuse verifier");

        assert!(Arc::ptr_eq(&first, &second));
        let cache = sdk.inner.verifier_cache.lock().expect("verifier cache");
        assert_eq!(cache.len(), 1);
    }

    #[cfg(all(feature = "compile", feature = "verify", feature = "execute"))]
    #[test]
    fn prepare_verifier_build_failure_does_not_poison_cache() {
        let sdk = sdk_with_empty_runtime_host();
        let artifact = sdk.compile(SIMPLE_SOURCE).expect("compile simple source");

        let Err(first) = sdk.prepare_verifier(&artifact) else {
            panic!("verifier build must fail without host environment");
        };
        let Err(second) = sdk.prepare_verifier(&artifact) else {
            panic!("repeated verifier build failure must stay recoverable");
        };

        assert!(matches!(
            first,
            SdkError::Runtime(RuntimeError::ValidationFailed { .. })
        ));
        assert!(matches!(
            second,
            SdkError::Runtime(RuntimeError::ValidationFailed { .. })
        ));
        assert!(sdk.inner.verifier_cache.lock().is_ok());
    }
}
