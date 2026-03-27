//! STARK witness lowering for successful native transactions.

mod context;
mod driver;
pub mod ops;
mod slots;

pub use driver::{
    LowerSuccessfulTxInput, LoweringOutput, TxLoweringOutput, lower_successful_tx,
    merge_lowering_outputs,
};
