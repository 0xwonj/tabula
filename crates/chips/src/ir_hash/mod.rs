//! Dedicated proof lane for canonical IR hash semantics.

pub mod air;
pub mod call;
pub mod kit;
mod trace;

pub use air::IrHashChip;
pub use call::{
    IR_HASH_BUS, IR_HASH_CHIP_ID, IR_HASH_WITNESS_LABEL, IrHashCall, encode_ir_hash_payload,
};
pub use kit::IrHashKit;
