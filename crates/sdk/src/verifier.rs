use std::sync::{Arc, Mutex};

use tabula_artifact::Artifact;

use crate::Sdk;
use crate::error::SdkError;
use crate::proof::Proof;

/// Artifact-bound reusable verification object.
#[derive(Clone)]
pub struct Verifier {
    inner: Arc<VerifierInner>,
}

struct VerifierInner {
    sdk: Sdk,
    artifact: Artifact,
    verifier: Mutex<Option<Arc<tabula_runtime::Verifier>>>,
}

impl Verifier {
    pub(crate) fn new(sdk: Sdk, artifact: Artifact) -> Self {
        Self {
            inner: Arc::new(VerifierInner {
                sdk,
                artifact,
                verifier: Mutex::new(None),
            }),
        }
    }

    /// Eagerly prepare the bound verifier.
    pub fn warm(&self) -> Result<(), SdkError> {
        let _ = self.lower()?;
        Ok(())
    }

    /// Verify one proof against this verifier's bound artifact.
    pub fn verify(&self, proof: &Proof) -> Result<(), SdkError> {
        self.lower()?.verify(&proof.proof, &proof.statement)?;
        Ok(())
    }

    fn lower(&self) -> Result<Arc<tabula_runtime::Verifier>, SdkError> {
        let mut verifier = self
            .inner
            .verifier
            .lock()
            .expect("sdk verifier mutex poisoned");
        if let Some(verifier) = verifier.as_ref() {
            return Ok(Arc::clone(verifier));
        }

        let built = Arc::new(self.inner.sdk.build_verifier(&self.inner.artifact)?);
        *verifier = Some(Arc::clone(&built));
        Ok(built)
    }
}

impl std::fmt::Debug for Verifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Verifier")
            .field("artifact", &self.inner.artifact)
            .finish_non_exhaustive()
    }
}
