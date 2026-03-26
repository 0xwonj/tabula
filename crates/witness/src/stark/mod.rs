//! STARK-specific witness lowering and witness-store assembly helpers.

pub mod execution_store;
pub mod lowering;
mod memory;
mod root_paths;
pub mod root_store;
mod rows;
pub mod schemes;

pub(crate) use rows::{AccessRow, InitRow};

pub use execution_store::prepare_execution_store;
pub use lowering::{
    LowerSuccessfulTxInput, LoweringOutput, TxLoweringOutput, lower_successful_tx,
    merge_lowering_outputs,
};
pub use root_store::{SmtRootStoreContext, prepare_smt_root_store};
