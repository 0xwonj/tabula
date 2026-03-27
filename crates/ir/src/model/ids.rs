//! Stable numeric identifiers for IR model entities.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tabula_core::TypeId;

/// A resolved type reference (alias for [`TypeId`]).
pub type TypeRef = TypeId;

/// Identifies a program registered in the Tabula registry.
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
pub struct ProgramId(pub u32);

/// Identifies a callable entry (function, query, or transaction) within a program.
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
pub struct EntryId(pub u32);

/// Identifies a parameter of a callable entry.
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
pub struct ParamId(pub u32);

/// Identifies a local variable within an entry body.
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
pub struct LocalId(pub u32);

/// Identifies a field in the program's public context.
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
pub struct ContextFieldId(pub u32);

/// Identifies a compile-time constant in the constant pool.
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
pub struct ConstId(pub u32);

/// Identifies a state table.
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
pub struct TableId(pub u32);

/// Identifies a column field within a state table.
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
pub struct FieldId(pub u16);

/// Identifies a static relation.
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
pub struct RelationId(pub u32);

/// Identifies a native capability.
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
pub struct CapabilityId(pub u32);

/// Identifies an event type.
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
pub struct EventId(pub u32);

/// Discriminates callable entry kinds.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum EntryKind {
    /// A read-only query (no state writes).
    Query,
    /// A state-mutating transaction.
    Tx,
}

/// Whether a callable entry has an explicit return value.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum ReturnPolicy {
    /// The entry returns nothing (unit).
    Unit,
    /// The entry returns one or more explicit values.
    Explicit,
}

impl From<TableId> for tabula_core::TableId {
    fn from(value: TableId) -> Self {
        Self(value.0)
    }
}

impl From<FieldId> for tabula_core::ColId {
    fn from(value: FieldId) -> Self {
        Self(value.0)
    }
}
