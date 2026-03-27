//! Callable entry declarations.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use super::{Body, EntryId, EntryKind, LocalId, ParamId, ReturnPolicy, TypeRef};

/// A callable entry: a function, query, or transaction.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Entry {
    /// Unique entry identifier within the program.
    pub id: EntryId,
    /// Source-level name.
    pub symbol: String,
    /// Whether this is a query or transaction.
    pub kind: EntryKind,
    /// Parameter declarations.
    pub params: Vec<ParamDecl>,
    /// Return value types.
    pub returns: Vec<TypeRef>,
    /// How the return value is communicated.
    pub return_policy: ReturnPolicy,
    /// Executable body.
    pub body: Body,
}

/// A parameter declaration for a callable entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ParamDecl {
    /// Unique parameter identifier within the entry.
    pub id: ParamId,
    /// Source-level parameter name.
    pub symbol: String,
    /// Parameter type.
    pub ty: TypeRef,
}

/// A local variable declaration within an entry body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct LocalDecl {
    /// Unique local identifier within the entry body.
    pub id: LocalId,
    /// Type of the local variable.
    pub ty: TypeRef,
}
