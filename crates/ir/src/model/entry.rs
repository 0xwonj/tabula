use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use super::{Body, EntryId, EntryKind, LocalId, ParamId, ReturnPolicy, TypeRef};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Entry {
    pub id: EntryId,
    pub symbol: String,
    pub kind: EntryKind,
    pub params: Vec<ParamDecl>,
    pub returns: Vec<TypeRef>,
    pub return_policy: ReturnPolicy,
    pub body: Body,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ParamDecl {
    pub id: ParamId,
    pub symbol: String,
    pub ty: TypeRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct LocalDecl {
    pub id: LocalId,
    pub ty: TypeRef,
}
