use serde::{Deserialize, Serialize};
use tabula_ir::EntryBatch;

/// Portable transaction batch on the SDK happy path.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TransactionBatch(pub(crate) EntryBatch);

impl TransactionBatch {
    pub(crate) fn from_raw(raw: EntryBatch) -> Self {
        Self(raw)
    }

    pub(crate) fn as_raw(&self) -> &EntryBatch {
        &self.0
    }
}
