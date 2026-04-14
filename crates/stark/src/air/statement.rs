//! Structured public statement for Tabula batch proofs.
//!
//! [`PublicStatement`] replaces raw `Vec<KoalaBear>` + magic offset constants
//! with a typed representation of what the proof asserts.

use p3_koala_bear::KoalaBear;
use tabula_commitment::NativeDigest;

/// Structured public statement for a Tabula batch proof.
///
/// Contains the state roots and source commitments that the proof asserts.
/// Serialized as 40 KoalaBear field elements for the STARK public values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicStatement {
    /// State root before the batch.
    pub old_root: NativeDigest,
    /// State root after the batch.
    pub new_root: NativeDigest,
    /// Poseidon commitment to the canonical public context payload.
    pub public_context_digest: NativeDigest,
    /// Poseidon commitment to the canonical transaction-batch payload.
    pub applied_tx_digest: NativeDigest,
    /// Poseidon commitment to the canonical emitted-event payload.
    pub event_digest: NativeDigest,
}

impl PublicStatement {
    const DIGEST_FIELD_ELEMENTS: usize = 8;

    /// Total number of field elements when serialized.
    pub const NUM_FIELD_ELEMENTS: usize = Self::DIGEST_FIELD_ELEMENTS * 5;

    /// Serialize to a flat field-element vector for AIR public values.
    pub fn to_field_elements(&self) -> Vec<KoalaBear> {
        let mut pvs = Vec::with_capacity(Self::NUM_FIELD_ELEMENTS);
        pvs.extend_from_slice(&self.old_root.0);
        pvs.extend_from_slice(&self.new_root.0);
        pvs.extend_from_slice(&self.public_context_digest.0);
        pvs.extend_from_slice(&self.applied_tx_digest.0);
        pvs.extend_from_slice(&self.event_digest.0);
        pvs
    }

    /// Deserialize from a flat field-element slice.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the slice length does not match [`NUM_FIELD_ELEMENTS`](Self::NUM_FIELD_ELEMENTS).
    pub fn from_field_elements(pvs: &[KoalaBear]) -> Result<Self, PublicStatementError> {
        if pvs.len() != Self::NUM_FIELD_ELEMENTS {
            return Err(PublicStatementError::WrongLength {
                expected: Self::NUM_FIELD_ELEMENTS,
                got: pvs.len(),
            });
        }
        let old_root = NativeDigest(core::array::from_fn(|i| pvs[i]));
        let new_root = NativeDigest(core::array::from_fn(|i| {
            pvs[Self::DIGEST_FIELD_ELEMENTS + i]
        }));
        let public_context_digest = NativeDigest(core::array::from_fn(|i| {
            pvs[Self::DIGEST_FIELD_ELEMENTS * 2 + i]
        }));
        let applied_tx_digest = NativeDigest(core::array::from_fn(|i| {
            pvs[Self::DIGEST_FIELD_ELEMENTS * 3 + i]
        }));
        let event_digest = NativeDigest(core::array::from_fn(|i| {
            pvs[Self::DIGEST_FIELD_ELEMENTS * 4 + i]
        }));
        Ok(Self {
            old_root,
            new_root,
            public_context_digest,
            applied_tx_digest,
            event_digest,
        })
    }
}

/// Errors when parsing a [`PublicStatement`] from field elements.
#[derive(Clone, Debug, thiserror::Error)]
pub enum PublicStatementError {
    /// Wrong number of field elements.
    #[error("expected {expected} field elements, got {got}")]
    WrongLength {
        /// Expected count.
        expected: usize,
        /// Actual count.
        got: usize,
    },
}
