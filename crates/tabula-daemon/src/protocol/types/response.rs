//! Generic API response wrapper.

use serde::Serialize;

/// Unified API response envelope. T is flattened into the top-level JSON.
#[derive(Debug, Clone, Serialize)]
pub struct ApiResponse<T: Serialize> {
    pub ok: bool,
    #[serde(flatten)]
    pub data: T,
}

impl<T: Serialize> ApiResponse<T> {
    /// Build a success response.
    pub fn ok(data: T) -> Self {
        Self { ok: true, data }
    }
}

/// Macro to create a thin named wrapper that serializes as `{ field_name: value }`.
macro_rules! named_response {
    ($name:ident, $field:ident, $ty:ty) => {
        #[derive(Debug, Clone, ::serde::Serialize)]
        pub struct $name {
            pub $field: $ty,
        }
    };
}

use crate::service::{InstanceRecord, ProgramRecord, RunRecord};

named_response!(ProgramResponse, program, ProgramRecord);
named_response!(ProgramListResponse, programs, Vec<ProgramRecord>);
named_response!(InstanceResponse, instance, InstanceRecord);
named_response!(InstanceListResponse, instances, Vec<InstanceRecord>);
named_response!(RunResponse, run, RunRecord);
named_response!(RunListResponse, runs, Vec<RunRecord>);
