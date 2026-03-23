//! IR definitions for the Tabula kernel: instructions, tx types, and program validation.

pub mod instruction;
pub mod pass;
pub mod program;
pub mod tx;

pub use instruction::{
    AggregateKind, ArithOp, CmpOp, GENERIC_EXECUTION_VALUE_WIDTH, Instruction, PrecompileId,
    PrecompileSignature, PrecompileValueProfile, PropertyQuery, PropertyRequirement, RowExpr, Slot,
    ValueExpr,
};
pub use pass::BodyTypeInfo;
pub use program::Program;
pub use tabula_core::PropertyQueryKind;
pub use tx::{ParamDef, TxTypeDef};
