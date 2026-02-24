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
#[macro_use]
pub mod bus_macro;
pub mod bus;
pub mod chip_instance;
pub mod chip_set;
pub mod chips;
pub mod columns;
pub mod debug;
pub mod extractor;
pub mod gadgets;
pub mod interaction;

pub use crate::trace_builder::TraceGenerator;
pub use builder::{EmptyMessageBuilder, InteractionAirBuilder};
pub use chip_instance::ChipInstance;
pub use chip_set::ChipSet;
pub use chips::column_meta::trace::ColumnMetaInput;
pub use chips::column_meta::{
    COLUMN_META_WIDTH, ColumnMetaChip, ColumnMetaCols, generate_column_meta_trace,
};
pub use chips::execution::{
    CmpOp, HASH_INSTRUCTION_DOMAIN_TAG, HASH_INSTRUCTION_INPUT_COUNT, InstructionRecord, Opcode,
    limbs_to_u64, u64_to_limbs,
};
pub use chips::inter_tx_order::{
    INTER_TX_ORDER_STANDARD_WIDTH, InterTxOrderChip, InterTxOrderCols, InterTxOrderRow,
    generate_inter_tx_order_trace, inter_tx_order_width,
};
pub use chips::poseidon::POSEIDON_PERM_WIDTH;
pub use chips::range_check::{
    RangeCheckChip, generate_range_check_preprocessed, generate_range_check_trace,
};
pub use chips::smt_path::{
    SMT_COL_PATH_WIDTH, SMT_TABLE_PATH_NEW_ROOT_PV_OFFSET, SMT_TABLE_PATH_NUM_PUBLIC_VALUES,
    SMT_TABLE_PATH_OLD_ROOT_PV_OFFSET, SMT_TABLE_PATH_WIDTH, SmtColPathChip, SmtPathCols,
    SmtPathWitness, SmtTablePathChip, SmtTablePathCols, SmtTablePathWitness,
    generate_smt_col_path_trace, generate_smt_table_path_trace,
};
pub use chips::state_column::{
    STATE_COLUMN_STANDARD_WIDTH, StateColumnChip, StateColumnCols, StateColumnRow,
    generate_state_column_trace, state_column_width,
};
pub use chips::static_table::{
    STATIC_TABLE_STANDARD_WIDTH, StaticTableChip, StaticTableCols, StaticTableRow,
    generate_static_table_trace, static_table_width,
};
pub use chips::{ChipSpec, TabulaAir};
pub use columns::{borrow_cols, borrow_cols_mut, num_cols};
pub use debug::{
    ChipRecord, ChipTrace, check_bus_balance, check_logup_balance, check_public_input_binding,
    debug_check, debug_check_all, debug_check_logup, debug_check_with_preprocessed,
    debug_check_with_preprocessed_and_public_values, debug_check_with_public_values, evaluate_chip,
    evaluate_chip_with_preprocessed, evaluate_chip_with_preprocessed_and_public_values,
    evaluate_chip_with_public_values,
};
pub use extractor::{count_interactions, extract_interactions};
pub use gadgets::bool_fe;
pub use gadgets::{
    IsZero, LimbHalves, StrictIneq, U64Limbs, constrain_is_real_prefix, constrain_is_zero,
    constrain_limb_halves, constrain_null_canon, constrain_strict_ineq,
    constrain_u64_decomposition,
};
pub use interaction::{AirInteraction, InteractionKind};
