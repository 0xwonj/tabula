use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tabula_core::PortableValue;

use super::{CapabilityId, EventId, RelationId, TypeRef};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RelationManifest {
    pub entries: Vec<RelationManifestEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RelationManifestEntry {
    pub id: RelationId,
    pub descriptor: RelationDescriptor,
    pub binding: RelationBinding,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RelationDescriptor {
    pub symbol: String,
    pub inputs: Vec<TypeRef>,
    pub outputs: Vec<TypeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum RelationBinding {
    EnumSet { values: Vec<PortableValue> },
    Map { rows: Vec<RelationRow> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RelationRow {
    pub inputs: Vec<PortableValue>,
    pub outputs: Vec<PortableValue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct EventManifest {
    pub entries: Vec<EventDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct EventDescriptor {
    pub id: EventId,
    pub symbol: String,
    pub fields: Vec<TypeRef>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CapabilityManifest {
    pub entries: Vec<CapabilityDescriptor>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CapabilityDescriptor {
    pub id: CapabilityId,
    pub symbol: String,
    pub inputs: Vec<TypeRef>,
    pub outputs: Vec<TypeRef>,
    pub totality: CapabilityTotality,
    pub query_policy: CapabilityQueryPolicy,
    pub proof_visibility: CapabilityProofVisibility,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum CapabilityTotality {
    Total,
    Checked,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum CapabilityQueryPolicy {
    QuerySafe,
    TxOnly,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum CapabilityProofVisibility {
    Journaled,
    OpaqueRuntimeOnly,
}
