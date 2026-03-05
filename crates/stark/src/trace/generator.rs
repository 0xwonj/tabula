//! `TraceGenerator` trait — standardized trace production for AIR chips.
//!
//! Each chip defines its `Input` type and implements trace generation.
//! The default `build_entry()` composes main + optional preprocessed
//! into a complete [`TraceEntry`].

use p3_baby_bear::BabyBear;
use p3_matrix::dense::RowMajorMatrix;

use crate::chips::ChipSpec;

use super::trace_map::TraceEntry;

/// Trait for chips that can generate their own trace matrices.
///
/// Each chip defines its `Input` type and implements trace generation.
/// The default `build_entry()` composes main + optional preprocessed
/// into a complete [`TraceEntry`].
pub trait TraceGenerator: ChipSpec {
    /// The input data this chip needs.
    type Input: ?Sized;

    /// Generate the main execution trace.
    fn generate_trace(&self, input: &Self::Input) -> RowMajorMatrix<BabyBear>;

    /// Generate preprocessed trace (round constants, etc.). Default: none.
    fn generate_preprocessed(&self, _input: &Self::Input) -> Option<RowMajorMatrix<BabyBear>> {
        None
    }

    /// Build a complete TraceEntry (main + optional preprocessed).
    fn build_entry(&self, input: &Self::Input) -> TraceEntry {
        TraceEntry {
            main: self.generate_trace(input),
            preprocessed: self.generate_preprocessed(input),
            public_values: Vec::new(),
        }
    }
}
