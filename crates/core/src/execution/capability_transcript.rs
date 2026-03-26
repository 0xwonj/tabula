//! Shared typed contract for capability transcript materialization.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use crate::{EncodingProfileId, TypeId};

/// Stable identifier for one capability transcript family.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct CapabilityTranscriptId(pub u16);

/// One typed value slot contract for transcript materialization.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct CapabilityTranscriptValueProfile {
    /// Semantic runtime type expected at this slot.
    pub type_id: TypeId,
    /// Runtime encoding profile expected for transcript/proof materialization.
    pub encoding_profile_id: EncodingProfileId,
}

/// Sealed typed I/O contract for one capability transcript family.
#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
)]
pub struct CapabilityTranscriptSignature {
    /// Ordered input slots.
    pub inputs: Vec<CapabilityTranscriptValueProfile>,
    /// Ordered output slots.
    pub outputs: Vec<CapabilityTranscriptValueProfile>,
}

impl CapabilityTranscriptSignature {
    /// Build one typed signature from ordered input/output value profiles.
    #[must_use]
    pub fn new(
        inputs: Vec<CapabilityTranscriptValueProfile>,
        outputs: Vec<CapabilityTranscriptValueProfile>,
    ) -> Self {
        Self { inputs, outputs }
    }
}
