use std::sync::Arc;

#[cfg(feature = "prove")]
use std::sync::Mutex;

use tabula_artifact::{Artifact, State, TransactionBatch};
use tabula_compiler::SealedProgram;
use tabula_core::Digest;
use tabula_core::traits::Hasher;
use tabula_runtime::{CompiledBatchInput, run_compiled_batch};

#[cfg(feature = "prove")]
use crate::Proof;
use crate::Sdk;
use crate::error::SdkError;
use crate::execution::Execution;
#[cfg(feature = "verify")]
use crate::verifier::Verifier;

/// User-facing reusable program object.
#[derive(Clone)]
pub struct Program {
    inner: Arc<ProgramInner>,
}

struct ProgramInner {
    #[cfg(any(feature = "prove", feature = "verify"))]
    sdk: Sdk,
    sealed: SealedProgram,
    artifact: Artifact,
    #[cfg(feature = "prove")]
    program_hash: String,
    #[cfg(feature = "prove")]
    runtime: Mutex<Option<Arc<tabula_runtime::TabulaRuntime>>>,
}

impl Program {
    pub(crate) fn from_compiled(sdk: Sdk, sealed: SealedProgram) -> Result<Self, SdkError> {
        let artifact = sealed.as_artifact();
        Self::from_artifact(sdk, sealed, artifact)
    }

    pub(crate) fn from_artifact(
        sdk: Sdk,
        sealed: SealedProgram,
        artifact: Artifact,
    ) -> Result<Self, SdkError> {
        #[cfg(not(any(feature = "prove", feature = "verify")))]
        let _ = &sdk;
        #[cfg(feature = "prove")]
        let program_hash = artifact.canonical_digest()?;
        Ok(Self {
            inner: Arc::new(ProgramInner {
                #[cfg(any(feature = "prove", feature = "verify"))]
                sdk,
                sealed,
                artifact,
                #[cfg(feature = "prove")]
                program_hash,
                #[cfg(feature = "prove")]
                runtime: Mutex::new(None),
            }),
        })
    }

    /// The sealed artifact for this program.
    pub fn artifact(&self) -> &Artifact {
        &self.inner.artifact
    }

    /// Execute one batch against one state.
    pub fn execute(&self, state: &State, batch: &TransactionBatch) -> Result<Execution, SdkError> {
        match run_compiled_batch(&CompiledBatchInput {
            compiled_program: &self.inner.sealed,
            state,
            batch,
            hasher: &SdkBlake3Hasher,
        }) {
            Ok(executed) => {
                #[cfg(feature = "prove")]
                {
                    Ok(Execution::new(
                        self.inner.program_hash.clone(),
                        state.clone(),
                        batch.clone(),
                        executed,
                    ))
                }

                #[cfg(not(feature = "prove"))]
                {
                    Ok(Execution::new(executed))
                }
            }
            Err(err) if self.requires_prepared_runtime() => {
                self.execute_with_prepared_runtime(state, batch, err)
            }
            Err(err) => Err(SdkError::from(err)),
        }
    }

    /// Create an artifact-bound verifier using this program's SDK context.
    #[cfg(feature = "verify")]
    pub fn verifier(&self) -> Result<Verifier, SdkError> {
        self.inner.sdk.verifier(self.inner.artifact.clone())
    }

    /// Eagerly prepare the proving runtime.
    #[cfg(feature = "prove")]
    pub fn warm(&self) -> Result<(), SdkError> {
        let _ = self.runtime()?;
        Ok(())
    }

    /// Prove one previously executed batch.
    #[cfg(feature = "prove")]
    pub fn prove(&self, execution: &Execution) -> Result<Proof, SdkError> {
        if execution.program_hash != self.inner.program_hash {
            return Err(SdkError::ExecutionProgramMismatch);
        }

        let result = self.runtime()?.prove(&tabula_runtime::ProveInput {
            state: &execution.state,
            batch: &execution.batch,
            executed: &execution.inner,
        })?;
        Ok(Proof::from_prove_result(result))
    }

    /// Execute, prove, and return the resulting proof bundle.
    #[cfg(feature = "prove")]
    pub fn execute_and_prove(
        &self,
        state: &State,
        batch: &TransactionBatch,
    ) -> Result<Proof, SdkError> {
        let execution = self.execute(state, batch)?;
        self.prove(&execution)
    }

    fn requires_prepared_runtime(&self) -> bool {
        !self.inner.sealed.precompile_manifest().is_empty()
            || !self
                .inner
                .sealed
                .required_property_requirements()
                .is_empty()
    }

    fn execute_with_prepared_runtime(
        &self,
        state: &State,
        batch: &TransactionBatch,
        _free_error: tabula_runtime::RuntimeError,
    ) -> Result<Execution, SdkError> {
        #[cfg(feature = "prove")]
        {
            let executed = self.runtime()?.execute(state, batch)?;
            Ok(Execution::new(
                self.inner.program_hash.clone(),
                state.clone(),
                batch.clone(),
                executed,
            ))
        }

        #[cfg(not(feature = "prove"))]
        {
            let _ = (state, batch);
            Err(SdkError::FeatureDisabled {
                feature: "prove",
                detail: "capability-backed execution requires the `prove` feature".to_string(),
            })
        }
    }

    #[cfg(feature = "prove")]
    fn runtime(&self) -> Result<Arc<tabula_runtime::TabulaRuntime>, SdkError> {
        let mut runtime = self
            .inner
            .runtime
            .lock()
            .expect("program runtime mutex poisoned");
        if let Some(runtime) = runtime.as_ref() {
            return Ok(Arc::clone(runtime));
        }

        let built = Arc::new(self.inner.sdk.build_runtime(&self.inner.sealed)?);
        *runtime = Some(Arc::clone(&built));
        Ok(built)
    }
}

struct SdkBlake3Hasher;

impl Hasher for SdkBlake3Hasher {
    fn hash(&self, data: &[u8]) -> Digest {
        *blake3::hash(data).as_bytes()
    }

    fn hash_pair(&self, left: &Digest, right: &Digest) -> Digest {
        let mut hasher = blake3::Hasher::new();
        hasher.update(left);
        hasher.update(right);
        *hasher.finalize().as_bytes()
    }
}

impl std::fmt::Debug for Program {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Program")
            .field("artifact", &self.inner.artifact)
            .finish_non_exhaustive()
    }
}
