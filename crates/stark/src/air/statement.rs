//! Structured public statement for Tabula batch proofs.
//!
//! [`PublicStatement`] replaces raw `Vec<BabyBear>` + magic offset constants
//! with a typed representation of what the proof asserts.

use p3_baby_bear::BabyBear;
use tabula_commitment::NativeDigest;

/// Structured public statement for a Tabula batch proof.
///
/// Contains the state roots that the proof asserts transition between.
/// Serialized as 16 BabyBear field elements for the STARK public values.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PublicStatement {
    /// State root before the batch.
    pub old_root: NativeDigest,
    /// State root after the batch.
    pub new_root: NativeDigest,
}

impl PublicStatement {
    /// Total number of field elements when serialized.
    pub const NUM_FIELD_ELEMENTS: usize = 16; // 8 + 8

    /// Serialize to a flat field-element vector for AIR public values.
    pub fn to_field_elements(&self) -> Vec<BabyBear> {
        let mut pvs = Vec::with_capacity(Self::NUM_FIELD_ELEMENTS);
        pvs.extend_from_slice(&self.old_root.0);
        pvs.extend_from_slice(&self.new_root.0);
        pvs
    }

    /// Deserialize from a flat field-element slice.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the slice length does not match [`NUM_FIELD_ELEMENTS`](Self::NUM_FIELD_ELEMENTS).
    pub fn from_field_elements(pvs: &[BabyBear]) -> Result<Self, PublicStatementError> {
        if pvs.len() != Self::NUM_FIELD_ELEMENTS {
            return Err(PublicStatementError::WrongLength {
                expected: Self::NUM_FIELD_ELEMENTS,
                got: pvs.len(),
            });
        }
        let old_root = NativeDigest(core::array::from_fn(|i| pvs[i]));
        let new_root = NativeDigest(core::array::from_fn(|i| pvs[8 + i]));
        Ok(Self { old_root, new_root })
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
