//! Dedicated proof lane for static canonical relation tables.

pub mod air;
pub mod kit;
pub mod rows;
mod trace;

pub use air::RelationTableChip;
pub use kit::RelationTableKit;
pub use rows::{
    RELATION_TABLE_BUS, RELATION_TABLE_CHIP_ID, RELATION_TABLE_WITNESS_LABEL,
    RelationTableWitnessRow,
};
