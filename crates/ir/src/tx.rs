//! Transaction type definitions: the IR-level schema for transaction bodies.

use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

use tabula_core::{TxTypeId, TypeId};

use crate::Instruction;

/// A transaction type definition (part of the program).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct TxTypeDef {
    /// Transaction type identifier.
    pub id: TxTypeId,
    /// Human-readable name.
    pub name: String,
    /// Parameter schema.
    pub param_schema: Vec<ParamDef>,
    /// Tabula IR body.
    pub body: Vec<Instruction>,
}

/// Describes a parameter of a transaction type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
pub struct ParamDef {
    /// Parameter name.
    pub name: String,
    /// Sealed semantic type identifier used by generic IR and compiler logic.
    pub type_id: TypeId,
}
