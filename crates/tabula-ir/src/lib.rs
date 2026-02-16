#![warn(missing_docs)]
#![deny(unused)]

//! IR definitions for the Tabula kernel: instructions, tx types, and program validation.

pub mod instruction;
pub mod pass;
pub mod program;
pub mod tx;

pub use instruction::{ArithOp, CmpOp, Instruction, RowExpr, Slot, ValueExpr};
pub use pass::BodyTypeInfo;
pub use program::Program;
pub use tx::{ParamDef, TxTypeDef};
