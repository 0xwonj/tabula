//! SmtPathChip — Merkle path verification for the two-level SMT state root.
//!
//! Two chip variants:
//! - **SmtColPathChip**: column-level paths (C15 receive at leaf → C16 send at root)
//! - **SmtTablePathChip**: table-level paths (C16 receive at leaf → public input at root)

pub mod air;
pub mod columns;
pub mod trace;

pub use air::{
    SMT_TABLE_PATH_NEW_ROOT_PV_OFFSET, SMT_TABLE_PATH_NUM_PUBLIC_VALUES,
    SMT_TABLE_PATH_OLD_ROOT_PV_OFFSET, SmtColPathChip, SmtTablePathChip,
};
pub use columns::{
    DIGEST_WIDTH, SMT_COL_PATH_WIDTH, SMT_TABLE_PATH_WIDTH, SmtPathCols, SmtTablePathCols,
};
pub use trace::{
    SmtPathWitness, SmtTablePathWitness, generate_smt_col_path_trace, generate_smt_table_path_trace,
};
