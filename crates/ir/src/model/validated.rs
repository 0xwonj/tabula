use super::Program;
use borsh::{BorshDeserialize, BorshSerialize};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, BorshSerialize, BorshDeserialize)]
#[serde(transparent)]
pub struct ValidatedProgram(pub(crate) Program);

impl ValidatedProgram {
    pub fn as_program(&self) -> &Program {
        &self.0
    }

    pub fn into_program(self) -> Program {
        self.0
    }
}
