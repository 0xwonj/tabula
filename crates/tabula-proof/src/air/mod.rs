//! AIR constraint infrastructure for the Tabula proof system.
//!
//! Provides:
//! - Column struct utilities (`columns`) — zero-copy borrow of trace slices
//! - Interaction bus types (`bus`) — named LogUp channels
//! - Reusable constraint gadgets (`gadgets`) — `is_real` prefix, boolean, integer, memory
//! - Debug constraint checker (`debug`) — verify constraints without a prover
//! - Chip implementations (`chips`) — per-chip `BaseAir` + `Air`

pub mod bus;
pub mod chips;
pub mod columns;
pub mod debug;
pub mod gadgets;

pub use chips::column_meta::{
    COLUMN_META_WIDTH, ColumnMetaChip, ColumnMetaCols, generate_column_meta_trace,
};
pub use chips::range_check::{RangeCheckChip, generate_range_check_preprocessed};
pub use chips::sorted_mem::{
    GlobalSortedMemChip, GlobalSortedMemCols, SORTED_MEM_STANDARD_WIDTH, SortedMemRow,
    generate_sorted_mem_trace, sorted_mem_width,
};
pub use chips::{ChipMeta, TabulaAir};
pub use debug::{debug_check, debug_check_all};
pub use gadgets::{
    IsZero, StrictIneq, U64Limbs, constrain_is_real_prefix, constrain_is_zero,
    constrain_null_canon, constrain_strict_ineq, constrain_u64_decomposition,
};
