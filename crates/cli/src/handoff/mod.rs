//! Workflow-critical CLI handoff files.

mod receipt_bridge;

pub(crate) use receipt_bridge::{bridge_from_receipt, encode_receipt_bridge};
#[cfg(feature = "prove")]
pub(crate) use receipt_bridge::{decode_receipt_bridge, sdk_receipt_from_bridge};
