//! Entry body types: locals, instructions, and value references.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};
use tabula_core::PortableValue;

use super::{ConstId, ContextFieldId, LocalDecl, LocalId, Op, ParamId};

/// The executable body of a callable entry: local variable declarations and instructions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct Body {
    /// Local variable declarations (type-annotated slots).
    pub locals: Vec<LocalDecl>,
    /// Instruction sequence.
    pub ops: Vec<Op>,
}

/// A reference to a single value within an entry execution context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub enum ValueRef {
    /// An inline portable constant literal.
    Literal(PortableValue),
    /// A caller-supplied entry parameter.
    Param(ParamId),
    /// A public context field supplied by the caller context.
    Context(ContextFieldId),
    /// A local variable produced by a previous instruction.
    Local(LocalId),
    /// A compile-time constant from the constant pool.
    Const(ConstId),
}

/// An ordered tuple of value references (used for multi-value instruction operands).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ValueTupleRef(pub Vec<ValueRef>);

/// A boolean local variable that gates a fallible operation.
///
/// When present on an instruction, the operation is skipped (not executed) if the
/// guard local evaluates to `false` at runtime.
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
