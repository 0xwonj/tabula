//! StateColumn AIR chip — unified SSMC + Merge.
//!
//! Replaces GlobalSSMC (sorted-set membership commitment) and GlobalMerge
//! (3-way merge proof) with a single chip that maintains two parallel hash
//! chains (Com_old and Com_new) over sorted entries per `(t,c)` segment.
//!
//! Row types: entry (old_only/write_only/both/delete) and gap (non-membership).

pub mod air;
mod buses;
pub mod columns;
mod derived;
pub mod trace;

pub use air::StateColumnChip;
pub use columns::{STATE_COLUMN_STANDARD_WIDTH, StateColumnCols, state_column_width};
pub use trace::{StateColumnRow, generate_state_column_trace};
