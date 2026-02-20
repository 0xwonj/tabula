//! AIR constraint infrastructure for the Tabula proof system.
//!
//! Provides:
//! - Column struct utilities (`columns`) — zero-copy borrow of trace slices
//! - Cross-chip interaction types (`interaction`) — LogUp bus definitions
//! - Interaction builder trait (`builder`) — send/receive during `eval()`
//! - Reusable constraint gadgets (`gadgets`) — `is_real` prefix, boolean, integer, memory
//! - Debug constraint checker (`debug`) — verify constraints and LogUp balance
//! - Chip implementations (`chips`) — per-chip `BaseAir` + `Air`

pub mod builder;
pub mod bus;
pub mod chips;
pub mod columns;
pub mod debug;
pub mod gadgets;
pub mod interaction;

pub use builder::InteractionAirBuilder;
pub use chips::column_meta::{
    COLUMN_META_WIDTH, ColumnMetaChip, ColumnMetaCols, generate_column_meta_trace,
};
pub use chips::execution::{
    CmpOp, HASH_INSTRUCTION_DOMAIN_TAG, HASH_INSTRUCTION_INPUT_COUNT, InstructionRecord, Opcode,
    u64_to_limbs,
};
pub use chips::merge::{MergeRow, MergeSource};
pub use chips::poseidon::POSEIDON_PERM_WIDTH;
pub use chips::range_check::{
    RangeCheckChip, generate_range_check_preprocessed, generate_range_check_trace,
};
pub use chips::sorted_mem::{
    GlobalSortedMemChip, GlobalSortedMemCols, SORTED_MEM_STANDARD_WIDTH, SortedMemRow,
    generate_sorted_mem_trace, sorted_mem_width,
};
pub use chips::ssmc::SsmcEntry;
pub use chips::{ChipMeta, TabulaAir};
pub use columns::{borrow_cols, borrow_cols_mut, num_cols};
pub use debug::{
    ChipRecord, ChipTrace, check_bus_balance, check_logup_balance, debug_check, debug_check_all,
    debug_check_logup, debug_check_with_preprocessed, evaluate_chip,
    evaluate_chip_with_preprocessed,
};
pub use gadgets::bool_fe;
pub use gadgets::{
    IsZero, LimbHalves, StrictIneq, U64Limbs, constrain_is_real_prefix, constrain_is_zero,
    constrain_limb_halves, constrain_null_canon, constrain_strict_ineq,
    constrain_u64_decomposition,
};
pub use interaction::{AirInteraction, InteractionKind};

// ── Bus builder traits ──
pub use bus::{
    CommitmentAirBuilder, MemoryAirBuilder, MergeAirBuilder, PoseidonAirBuilder,
    RangeCheckAirBuilder, SortedMemMetaAirBuilder, SsmcMembershipAirBuilder,
    StaticTableLookupAirBuilder,
};
