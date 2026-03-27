//! Stable numeric identifiers used throughout the HIR.

use tabula_core::TypeId;

/// A resolved type reference (alias for [`TypeId`]).
pub type TypeRef = TypeId;

/// Identifies a state table in the HIR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TableId(pub u32);

/// Identifies a column field within a state table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FieldId(pub u16);

/// Identifies a compile-time constant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ConstId(pub u32);

/// Identifies a static relation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RelationId(pub u32);

/// Identifies a callable (function, query, or transaction).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CallableId(pub u32);

/// Identifies a parameter of a callable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParamId(pub u32);

/// Identifies a native capability reference in a `use` declaration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CapabilityRefId(pub u32);

/// Identifies a local binding within a callable body.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BindingId(pub u32);

/// Identifies a field in the program's public context.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContextFieldId(pub u32);

/// Identifies an event type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EventId(pub u32);

/// Hash function family used by the `Hash` instruction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashFamily {
    /// Poseidon hash (STARK-friendly).
    Poseidon,
}

/// Whether a capability call can fail at runtime.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityTotality {
    /// The capability always succeeds.
    Total,
    /// The capability may return an error; callers must handle failure.
    Checked,
}

/// Whether a capability is safe to invoke from a read-only query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityQueryPolicy {
    /// Safe to call from queries (no state side-effects).
    QuerySafe,
    /// Only callable from transactions.
    TxOnly,
}

/// Whether capability effects are included in the proof journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapabilityProofVisibility {
    /// Effects are recorded in the execution journal and committed into the proof.
    Journaled,
    /// Effects are opaque; only the runtime sees them (not proven).
    OpaqueRuntimeOnly,
}
