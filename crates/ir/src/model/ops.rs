use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use super::{
    CapabilityId, EventId, FieldId, GuardRef, LocalId, RelationId, TableId, ValueRef, ValueTupleRef,
};

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum ArithOp {
    Add,
    Sub,
    Mul,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum CmpOp {
    Eq,
    Ne,
    Lt,
    Lte,
    Gt,
    Gte,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum HashFamily {
    Poseidon,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize,
)]
pub enum AggregateKind {
    Sum,
    Count,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum StatePropertyQuery {
    Minimum,
    Maximum,
    Successor {
        key: ValueTupleRef,
    },
    Predecessor {
        key: ValueTupleRef,
    },
    NonExistenceRange {
        lower: ValueTupleRef,
        upper: ValueTupleRef,
    },
    Aggregate {
        kind: AggregateKind,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum Op {
    Arith {
        dst: LocalId,
        op: ArithOp,
        lhs: ValueRef,
        rhs: ValueRef,
    },
    Cmp {
        dst: LocalId,
        op: CmpOp,
        lhs: ValueRef,
        rhs: ValueRef,
    },
    Not {
        dst: LocalId,
        src: ValueRef,
    },
    And {
        dst: LocalId,
        lhs: ValueRef,
        rhs: ValueRef,
    },
    Or {
        dst: LocalId,
        lhs: ValueRef,
        rhs: ValueRef,
    },
    Select {
        dst: LocalId,
        cond: ValueRef,
        if_true: ValueRef,
        if_false: ValueRef,
    },
    Hash {
        dst: LocalId,
        family: HashFamily,
        inputs: ValueTupleRef,
    },
    DivMod {
        guard: Option<GuardRef>,
        dst_q: LocalId,
        dst_r: LocalId,
        lhs: ValueRef,
        rhs: ValueRef,
    },
    ReadState {
        guard: Option<GuardRef>,
        dst_value: LocalId,
        dst_present: LocalId,
        table: TableId,
        key: ValueTupleRef,
        field: FieldId,
    },
    WriteState {
        guard: Option<GuardRef>,
        table: TableId,
        key: ValueTupleRef,
        field: FieldId,
        value: ValueRef,
    },
    DeleteState {
        guard: Option<GuardRef>,
        table: TableId,
        key: ValueTupleRef,
        field: FieldId,
    },
    ReadStateProperty {
        guard: Option<GuardRef>,
        dsts: Vec<LocalId>,
        table: TableId,
        field: FieldId,
        query: StatePropertyQuery,
    },
    Assert {
        guard: Option<GuardRef>,
        cond: ValueRef,
    },
    AssertRelation {
        guard: Option<GuardRef>,
        relation: RelationId,
        args: ValueTupleRef,
    },
    EvalRelation {
        guard: Option<GuardRef>,
        relation: RelationId,
        inputs: ValueTupleRef,
        dsts: Vec<LocalId>,
    },
    CallCapability {
        guard: Option<GuardRef>,
        capability: CapabilityId,
        inputs: ValueTupleRef,
        dsts: Vec<LocalId>,
    },
    EmitEvent {
        guard: Option<GuardRef>,
        event: EventId,
        args: ValueTupleRef,
    },
    Return {
        values: ValueTupleRef,
    },
}
