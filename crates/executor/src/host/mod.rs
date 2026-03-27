//! Host extension points: capability handlers and property read executors.

mod capability;
mod property_read;

pub use capability::{CapabilityExecutor, CapabilityHandler, CapabilityRegistry};
pub use property_read::{PropertyReadExecutor, PropertyReadQuery, PropertyReadRequest};
