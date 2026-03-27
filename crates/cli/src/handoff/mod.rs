//! Workflow-critical CLI handoff files.

mod receipt;

pub(crate) use receipt::{bridge_from_receipt, encode_receipt_bridge};
#[cfg(feature = "prove")]
pub(crate) use receipt::{decode_receipt_bridge, sdk_receipt_from_bridge};
