//! Execution journal value-types shared across the executor and downstream
//! layers (witness, runtime).
//!
//! These were historically owned by the executor crate; they are hosted here
//! so crates that consume executor outputs (like witness) need not depend on
//! the executor itself.

use tabula_core::{CommittedCellKey, CommittedPropertyQuery, TypeId};
use tabula_ir as ir;

use crate::{TypedCommittedPropertyQueryResult, TypedValue};

/// A single transaction call with its decoded parameter values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TxCall {
    /// Entry identifier for the transaction.
    pub entry_id: ir::EntryId,
    /// Decoded parameter values in declaration order.
    pub params: Vec<TypedValue>,
}

/// Whether a state effect is a read, write, or delete.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StateEffectKind {
    /// A state cell was read.
    Read,
    /// A state cell was written.
    Write,
    /// A state cell was deleted.
    Delete,
}

/// A single typed state cell access (read, write, or delete) within a transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedStateEffect {
    /// The cell key (table, column, row).
    pub key: CommittedCellKey,
    /// Type of the cell's value.
    pub type_id: TypeId,
    /// Whether this was a read, write, or delete.
    pub kind: StateEffectKind,
    /// The value involved (`None` for deletes, the old value for reads, new value for writes).
    pub value: Option<TypedValue>,
    /// Monotonically increasing logical clock at the time of this access.
    pub logical_time: u64,
    /// Index of the IR operation that produced this effect.
    pub op_index: usize,
    /// Ordinal of this effect among all effects within the enclosing entry execution.
    pub effect_ordinal_in_entry: u32,
}

/// A single state property read (structural query) within a transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatePropertyEffect {
    /// Target state table.
    pub table: ir::TableId,
    /// Target column field.
    pub field: ir::FieldId,
    /// The committed structural query that was evaluated.
    pub query: CommittedPropertyQuery,
    /// The committed-key-native result returned by the query.
    pub result: TypedCommittedPropertyQueryResult,
    /// Index of the IR operation that produced this effect.
    pub op_index: usize,
    /// Ordinal of this effect among all effects within the enclosing entry execution.
    pub effect_ordinal_in_entry: u32,
}

/// Whether a relation effect was an assertion check or an evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationEffectKind {
    /// A membership assertion (`AssertRelation`).
    Assert,
    /// An output-producing evaluation (`EvalRelation`).
    Eval,
}

/// A single static relation lookup within a transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationEffect {
    /// Target relation.
    pub relation: ir::RelationId,
    /// Whether this was an assertion or an evaluation.
    pub kind: RelationEffectKind,
    /// Input values supplied to the relation.
    pub inputs: Vec<TypedValue>,
    /// Output values returned by the relation (empty for assertions).
    pub outputs: Vec<TypedValue>,
    /// Index of the IR operation that produced this effect.
    pub op_index: usize,
    /// Ordinal of this effect among all effects within the enclosing entry execution.
    pub effect_ordinal_in_entry: u32,
}

/// A single event emission within a transaction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypedEventEffect {
    /// The event type that was emitted.
    pub event: ir::EventId,
    /// Field values for the emitted event.
    pub args: Vec<TypedValue>,
    /// Index of the IR operation that produced this effect.
    pub op_index: usize,
    /// Ordinal of this effect among all effects within the enclosing entry execution.
    pub effect_ordinal_in_entry: u32,
}
