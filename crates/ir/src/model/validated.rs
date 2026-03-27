//! Validated program wrapper.

use super::Program;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

/// A program that has passed compiler-side semantic validation.
///
/// `ValidatedProgram` is a newtype around [`Program`] that can only be
/// constructed by the compiler's validation pass.  Holding this type is proof
/// that the program IR satisfies all structural invariants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(transparent)]
pub struct ValidatedProgram(pub(crate) Program);

impl ValidatedProgram {
    /// Borrow the underlying program.
    pub fn as_program(&self) -> &Program {
        &self.0
    }

    /// Consume the wrapper and return the underlying program.
    pub fn into_program(self) -> Program {
        self.0
    }
}
