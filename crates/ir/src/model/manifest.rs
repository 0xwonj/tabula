//! Program manifests: relations, events, and native capabilities.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tabula_core::PortableValue;

use super::{CapabilityId, EventId, RelationId, TypeRef};

/// Complete static relation table manifest for a program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RelationManifest {
    /// All relations declared by the program.
    pub entries: Vec<RelationManifestEntry>,
}

/// A single static relation entry in the manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RelationManifestEntry {
    /// Unique relation identifier.
    pub id: RelationId,
    /// Human-readable descriptor (name and types).
    pub descriptor: RelationDescriptor,
    /// The static data bound to this relation.
    pub binding: RelationBinding,
}

/// Human-readable descriptor for a relation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RelationDescriptor {
    /// Source-level relation name.
    pub symbol: String,
    /// Input column types (in order).
    pub inputs: Vec<TypeRef>,
    /// Output column types (in order).
    pub outputs: Vec<TypeRef>,
}

/// The static data bound to a relation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum RelationBinding {
    /// A set of single-column input values (no outputs).
    EnumSet {
        /// The enumerated input values.
        values: Vec<PortableValue>,
    },
    /// An explicit (inputs → outputs) mapping.
    Map {
        /// All rows in the mapping.
        rows: Vec<RelationRow>,
    },
}

/// A single (inputs → outputs) row in a map relation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct RelationRow {
    /// Input values for this row.
    pub inputs: Vec<PortableValue>,
    /// Output values for this row.
    pub outputs: Vec<PortableValue>,
}

/// Complete event manifest for a program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct EventManifest {
    /// All events declared by the program.
    pub entries: Vec<EventDescriptor>,
}

/// Descriptor for a single event type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct EventDescriptor {
    /// Unique event identifier.
    pub id: EventId,
    /// Source-level event name.
    pub symbol: String,
    /// Event field types (in order).
    pub fields: Vec<TypeRef>,
}

/// Complete native capability manifest for a program.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CapabilityManifest {
    /// All native capabilities declared by the program.
    pub entries: Vec<CapabilityDescriptor>,
}

/// Descriptor for a single native capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct CapabilityDescriptor {
    /// Unique capability identifier.
    pub id: CapabilityId,
    /// Source-level capability name.
    pub symbol: String,
    /// Input parameter types (in order).
    pub inputs: Vec<TypeRef>,
    /// Output result types (in order).
    pub outputs: Vec<TypeRef>,
    /// Whether the capability can fail at runtime.
    pub totality: CapabilityTotality,
    /// Whether the capability is safe to invoke from a query.
    pub query_policy: CapabilityQueryPolicy,
    /// Whether capability effects are included in the proof journal.
    pub proof_visibility: CapabilityProofVisibility,
}

/// Whether a capability call can fail at runtime.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum CapabilityTotality {
    /// The capability always succeeds.
    Total,
    /// The capability may fail; callers must handle the error.
    Checked,
}

/// Whether a capability is safe to invoke from a read-only query.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum CapabilityQueryPolicy {
    /// Safe to call from queries (no state side-effects).
    QuerySafe,
    /// Only callable from transactions.
    TxOnly,
}

/// Whether capability effects are included in the proof journal.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum CapabilityProofVisibility {
    /// Effects are recorded in the execution journal and committed into the proof.
    Journaled,
    /// Effects are opaque; only the runtime sees them (not proven).
    OpaqueRuntimeOnly,
}
