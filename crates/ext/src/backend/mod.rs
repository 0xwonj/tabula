//! Advanced backend-facing support types for extension authors.

/// Backend-facing execution-tier extension contracts.
#[cfg(feature = "verify")]
pub mod execution;
/// Backend-facing semantic precompile proof authoring contracts.
#[cfg(feature = "verify")]
pub mod precompile;
/// Backend-facing column proof authoring contracts.
#[cfg(feature = "verify")]
pub mod scheme;

#[cfg(feature = "verify")]
pub use execution::ExecutionBackend;
#[cfg(feature = "verify")]
pub use tabula_machine::SetupError;
#[cfg(feature = "verify")]
pub use tabula_machine::backend::{AnyRap, ColumnChipSet, ProofColumn};
#[cfg(feature = "verify")]
pub use tabula_stark::chips::ChipIdAllocator;
#[cfg(feature = "verify")]
pub use tabula_stark::trace::{BusConsumer, DynChip, WitnessStore};

/// Convenience re-exports for low-level chip authoring.
pub mod prelude {
    #[cfg(feature = "verify")]
    pub use p3_air::{Air, AirBuilder, BaseAir, WindowAccess};
    #[cfg(feature = "verify")]
    pub use p3_field::PrimeCharacteristicRing;
    #[cfg(feature = "verify")]
    pub use p3_koala_bear::KoalaBear;
    #[cfg(feature = "verify")]
    pub use p3_matrix::Matrix;
    #[cfg(feature = "verify")]
    pub use p3_matrix::dense::RowMajorMatrix;
    #[cfg(feature = "prove")]
    pub use tabula_gadgets::integer::expr_from_u32;
    #[cfg(feature = "verify")]
    pub use tabula_machine::backend::prelude::*;
    #[cfg(feature = "verify")]
    pub use tabula_stark::air::columns::{borrow_cols, borrow_cols_mut};
    #[cfg(feature = "verify")]
    pub use tabula_stark::air::interaction::{AirInteraction, core_buses};
    #[cfg(feature = "verify")]
    pub use tabula_stark::trace::trace_map::TraceMap;
}
