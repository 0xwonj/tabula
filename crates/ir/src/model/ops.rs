//! IR instruction set: operations, operators, and property queries.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use super::{
    CapabilityId, EventId, FieldId, GuardRef, LocalId, RelationId, TableId, ValueRef, ValueTupleRef,
};

/// Arithmetic operator: addition, subtraction, or multiplication.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum ArithOp {
    /// Integer addition.
    Add,
    /// Integer subtraction.
    Sub,
    /// Integer multiplication.
    Mul,
}

/// Comparison operator.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum CmpOp {
    /// Equal.
    Eq,
    /// Not equal.
    Ne,
    /// Less than.
    Lt,
    /// Less than or equal.
    Lte,
    /// Greater than.
    Gt,
    /// Greater than or equal.
    Gte,
}

/// Hash function family used by the [`Op::Hash`] instruction.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum HashFamily {
    /// Poseidon hash (STARK-friendly, provable in-circuit).
    Poseidon,
}

/// Kind of aggregate property query over an ordered column.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum AggregateKind {
    /// Sum of all values in the column.
    Sum,
    /// Count of rows in the column.
    Count,
}

/// A structural property query over an ordered state column.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum StatePropertyQuery {
    /// The minimum key in the column.
    Minimum,
    /// The maximum key in the column.
    Maximum,
    /// The successor of a given key (next larger key).
    Successor {
        /// The reference key for which the successor is requested.
        key: ValueTupleRef,
    },
    /// The predecessor of a given key (next smaller key).
    Predecessor {
        /// The reference key for which the predecessor is requested.
        key: ValueTupleRef,
    },
    /// Prove that a key range contains no rows.
    NonExistenceRange {
        /// Inclusive lower bound of the empty range.
        lower: ValueTupleRef,
        /// Exclusive upper bound of the empty range.
        upper: ValueTupleRef,
    },
    /// An aggregate (sum or count) over the entire column.
    Aggregate {
        /// The aggregate function to apply.
        kind: AggregateKind,
    },
}

/// A single IR instruction within an entry body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum Op {
    /// Arithmetic: `dst = lhs op rhs`.
    Arith {
        /// Destination local.
        dst: LocalId,
        /// The arithmetic operator.
        op: ArithOp,
        /// Left operand.
        lhs: ValueRef,
        /// Right operand.
        rhs: ValueRef,
    },
    /// Comparison: `dst = lhs op rhs` (produces a boolean).
    Cmp {
        /// Destination local (boolean).
        dst: LocalId,
        /// The comparison operator.
        op: CmpOp,
        /// Left operand.
        lhs: ValueRef,
        /// Right operand.
        rhs: ValueRef,
    },
    /// Logical NOT: `dst = !src`.
    Not {
        /// Destination local (boolean).
        dst: LocalId,
        /// Source boolean value.
        src: ValueRef,
    },
    /// Logical AND: `dst = lhs && rhs`.
    And {
        /// Destination local (boolean).
        dst: LocalId,
        /// Left boolean operand.
        lhs: ValueRef,
        /// Right boolean operand.
        rhs: ValueRef,
    },
    /// Logical OR: `dst = lhs || rhs`.
    Or {
        /// Destination local (boolean).
        dst: LocalId,
        /// Left boolean operand.
        lhs: ValueRef,
        /// Right boolean operand.
        rhs: ValueRef,
    },
    /// Ternary select: `dst = if cond { if_true } else { if_false }`.
    Select {
        /// Destination local.
        dst: LocalId,
        /// Boolean condition.
        cond: ValueRef,
        /// Value when condition is true.
        if_true: ValueRef,
        /// Value when condition is false.
        if_false: ValueRef,
    },
    /// Cryptographic hash: `dst = hash(inputs)`.
    Hash {
        /// Destination local (hash digest).
        dst: LocalId,
        /// Hash function family to use.
        family: HashFamily,
        /// Values to hash.
        inputs: ValueTupleRef,
    },
    /// Division with remainder: `(dst_q, dst_r) = lhs / rhs`.
    DivMod {
        /// Optional guard; if present and false, the op is skipped.
        guard: Option<GuardRef>,
        /// Quotient destination.
        dst_q: LocalId,
        /// Remainder destination.
        dst_r: LocalId,
        /// Dividend.
        lhs: ValueRef,
        /// Divisor.
        rhs: ValueRef,
    },
    /// State read: `(dst_value, dst_present) = table[key].field`.
    ReadState {
        /// Optional guard.
        guard: Option<GuardRef>,
        /// Destination for the field value.
        dst_value: LocalId,
        /// Destination boolean indicating whether the row existed.
        dst_present: LocalId,
        /// Target table.
        table: TableId,
        /// Row key values.
        key: ValueTupleRef,
        /// Target column field.
        field: FieldId,
    },
    /// State write: `table[key].field = value`.
    WriteState {
        /// Optional guard.
        guard: Option<GuardRef>,
        /// Target table.
        table: TableId,
        /// Row key values.
        key: ValueTupleRef,
        /// Target column field.
        field: FieldId,
        /// Value to write.
        value: ValueRef,
    },
    /// State delete: remove a row field from the table.
    DeleteState {
        /// Optional guard.
        guard: Option<GuardRef>,
        /// Target table.
        table: TableId,
        /// Row key values.
        key: ValueTupleRef,
        /// Target column field.
        field: FieldId,
    },
    /// State property query: read a structural property of a column.
    ReadStateProperty {
        /// Optional guard.
        guard: Option<GuardRef>,
        /// Destination locals for the query outputs.
        dsts: Vec<LocalId>,
        /// Target table.
        table: TableId,
        /// Target column field.
        field: FieldId,
        /// The property query to evaluate.
        query: StatePropertyQuery,
    },
    /// Assert a boolean condition (aborts the transaction if false).
    Assert {
        /// Optional guard.
        guard: Option<GuardRef>,
        /// Boolean condition that must be true.
        cond: ValueRef,
    },
    /// Assert that a tuple satisfies a static relation.
    AssertRelation {
        /// Optional guard.
        guard: Option<GuardRef>,
        /// Target relation.
        relation: RelationId,
        /// Input arguments.
        args: ValueTupleRef,
    },
    /// Evaluate a static relation and capture its outputs.
    EvalRelation {
        /// Optional guard.
        guard: Option<GuardRef>,
        /// Target relation.
        relation: RelationId,
        /// Input arguments.
        inputs: ValueTupleRef,
        /// Destination locals for the relation outputs.
        dsts: Vec<LocalId>,
    },
    /// Invoke a native capability.
    CallCapability {
        /// Optional guard.
        guard: Option<GuardRef>,
        /// Target capability.
        capability: CapabilityId,
        /// Input arguments.
        inputs: ValueTupleRef,
        /// Destination locals for the capability outputs.
        dsts: Vec<LocalId>,
    },
    /// Emit an event.
    EmitEvent {
        /// Optional guard.
        guard: Option<GuardRef>,
        /// Target event type.
        event: EventId,
        /// Event field values.
        args: ValueTupleRef,
    },
    /// Return from the entry with optional values.
    Return {
        /// Return value references.
        values: ValueTupleRef,
    },
}
