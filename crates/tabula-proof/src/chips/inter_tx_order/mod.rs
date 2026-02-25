//! InterTxOrder AIR chip — inter-transaction access ordering.
//!
//! Mediates between Execution and StateColumn for conflicting batches
//! where multiple txs access the same `(t,c,r)`. Verifies inter-tx read
//! consistency at tx granularity (one row per (key, tx) pair).
//!
//! Row types: init (base state seed) and access (per-tx read/write).

pub mod air;
pub mod columns;
pub mod trace;

pub use air::InterTxOrderChip;
pub use columns::{INTER_TX_ORDER_STANDARD_WIDTH, InterTxOrderCols, inter_tx_order_width};
pub use trace::{InterTxOrderRow, generate_inter_tx_order_trace};
