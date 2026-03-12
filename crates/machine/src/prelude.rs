//! Convenience re-exports for extension authors.
//!
//! Import `tabula_machine::prelude::*` to get the types needed
//! for writing custom AIR chips and extensions.
//!
//! # What's included
//!
//! - **p3 types**: `Air`, `AirBuilder`, `BaseAir`, `BabyBear`,
//!   `PrimeCharacteristicRing`, `RowMajorMatrix`, `Matrix`
//! - **Tabula chip framework**: `ChipSpec`, `ChipId`, `ChipIdAllocator`,
//!   `BusId`, `InteractionAirBuilder`, `TraceContributor`, `TracePhase`,
//!   `WitnessStore`, `DynChip`
//! - **Machine-level types**: `AnyRap`, `ChipRegistry`, `ChipExtension`,
//!   `ExtensionContext`
//!
//! For bus declarations, use `tabula_stark::define_bus!`.

// p3 types for AIR constraint writing.
pub use p3_air::{Air, AirBuilder, BaseAir};
pub use p3_baby_bear::BabyBear;
pub use p3_field::PrimeCharacteristicRing;
pub use p3_matrix::Matrix;
pub use p3_matrix::dense::RowMajorMatrix;

// Tabula chip framework.
pub use tabula_stark::air::builder::InteractionAirBuilder;
pub use tabula_stark::air::interaction::BusId;
pub use tabula_stark::chips::{ChipId, ChipIdAllocator, ChipSpec};
pub use tabula_stark::trace::DynChip;
pub use tabula_stark::trace::column_commitment::BusConsumer;
pub use tabula_stark::trace::contributor::{TraceContributor, TracePhase, WitnessStore};
pub use tabula_stark::trace::trace_map::TraceMap;

// Machine-level types.
pub use crate::column_scheme::{ColumnChipSet, ColumnScheme};
pub use crate::extension::{ChipExtension, ExtensionContext};
pub use crate::property::{PropertyOpening, PropertyQuery, PropertyQueryKind, PropertyWitness};
pub use crate::{AnyRap, ChipRegistry};
