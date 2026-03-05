//! StaticTableChip — minimal LogUp receiver for static table lookups (C9).
//!
//! Each row provides one `(table_id, col_id, row_key, value)` tuple
//! that receives on the StaticTableLookup bus. Root binding deferred to M12.

pub mod air;
pub mod columns;
pub mod trace;

pub use air::StaticTableChip;
pub use columns::{STATIC_TABLE_STANDARD_WIDTH, StaticTableCols, static_table_width};
pub use trace::{StaticTableRow, generate_static_table_trace};
