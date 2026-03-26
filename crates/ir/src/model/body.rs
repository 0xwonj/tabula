use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tabula_core::PortableValue;

use super::{ConstId, ContextFieldId, LocalDecl, LocalId, Op, ParamId};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Body {
    pub locals: Vec<LocalDecl>,
    pub ops: Vec<Op>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum ValueRef {
    Literal(PortableValue),
    Param(ParamId),
    Context(ContextFieldId),
    Local(LocalId),
    Const(ConstId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ValueTupleRef(pub Vec<ValueRef>);

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
pub struct GuardRef(pub LocalId);
