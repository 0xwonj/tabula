//! STARK-specific witness lowering and witness-store assembly helpers.

pub mod execution_store;
pub mod lowering;
mod memory;
mod roots;
pub mod schemes;

pub(crate) use memory::rows::{AccessRow, InitRow};

pub use execution_store::prepare_execution_store;
pub use lowering::{
    ContextPreludeSlot, LowerSuccessfulTxInput, LoweringOutput, ParamPreludeSlot, TxLoweringOutput,
    lower_successful_tx, merge_lowering_outputs,
};
pub use roots::{SmtRootStoreContext, prepare_smt_root_store};
