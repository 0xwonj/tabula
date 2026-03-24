//! Convenience re-exports for backend-internal chip authoring.
//!
//! Import `tabula_machine::backend::prelude::*` to get the types needed
//! for writing backend AIR chips.
//!
//! # What's included
//!
//! - **p3 types**: `Air`, `AirBuilder`, `BaseAir`, `KoalaBear`,
//!   `PrimeCharacteristicRing`, `RowMajorMatrix`, `Matrix`
//! - **Tabula chip framework**: `ChipSpec`, `ChipId`, `ChipIdAllocator`,
//!   `BusId`, `InteractionAirBuilder`, `TraceContributor`, `TracePhase`,
//!   `WitnessStore`, `DynChip`
//! - **Machine backend types**: `AnyRap`, `ColumnChipSet`, `ProofColumn`
//!
//! For bus declarations, use `tabula_stark::define_bus!`.

pub use p3_air::{Air, AirBuilder, BaseAir};
pub use p3_field::PrimeCharacteristicRing;
pub use p3_koala_bear::KoalaBear;
pub use p3_matrix::Matrix;
pub use p3_matrix::dense::RowMajorMatrix;

pub use tabula_stark::air::builder::InteractionAirBuilder;
pub use tabula_stark::air::interaction::BusId;
pub use tabula_stark::chips::{ChipId, ChipIdAllocator, ChipSpec};
pub use tabula_stark::trace::DynChip;
pub use tabula_stark::trace::column_commitment::BusConsumer;
pub use tabula_stark::trace::contributor::{TraceContributor, TracePhase, WitnessStore};
pub use tabula_stark::trace::trace_map::TraceMap;

pub use crate::backend::{AnyRap, ColumnChipSet, ProofColumn};
