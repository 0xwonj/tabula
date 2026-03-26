use serde::{Deserialize, Serialize};
use tabula_ir::ContextInput;

/// Portable public context input on the SDK happy path.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Context(pub(crate) ContextInput);

impl Context {
    pub(crate) fn from_raw(raw: ContextInput) -> Self {
        Self(raw)
    }

    pub(crate) fn as_raw(&self) -> &ContextInput {
        &self.0
    }
}
