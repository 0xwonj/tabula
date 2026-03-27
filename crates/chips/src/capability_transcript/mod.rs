//! Generic transcript lane for capability calls.

pub mod air;
pub mod call;
mod trace;

pub use air::CapabilityTranscriptChip;
pub use call::{
    CAPABILITY_TRANSCRIPT_CHIP_ID, CAPABILITY_TRANSCRIPT_CONT_DOMAIN_TAG,
    CAPABILITY_TRANSCRIPT_FIRST_DOMAIN_TAG, CAPABILITY_TRANSCRIPT_WITNESS_LABEL,
    CapabilityCallHeader, CapabilityTranscriptCall, compute_capability_call_header,
    encode_capability_call_event_payload,
};
