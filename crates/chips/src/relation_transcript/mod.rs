//! Dedicated transcript lane for relation input/output tuples.

pub mod air;
pub mod call;
mod trace;

pub use air::RelationTranscriptChip;
pub use call::{
    RELATION_DIGEST_BUS, RELATION_TRANSCRIPT_CHIP_ID, RELATION_TRANSCRIPT_WITNESS_LABEL,
    RELATION_TUPLE_BUS, RelationTranscriptCall,
};
