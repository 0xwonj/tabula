//! AIR constraint framework for the Tabula proof system.
//!
//! Provides column struct utilities, cross-chip interaction types,
//! interaction builder trait, and chip set composition macro.

pub mod builder;
#[macro_use]
pub mod bus_macro;
pub mod bus;
pub mod chip_instance;
pub mod chip_set;
pub mod columns;
pub mod descriptor;
pub mod interaction;
pub mod keygen;
pub mod statement;

pub use builder::{EmptyMessageBuilder, InteractionAirBuilder};
pub use chip_instance::ChipInstance;
pub use chip_set::ChipSet;
pub use columns::{borrow_cols, borrow_cols_mut, num_cols};
pub use interaction::{AirInteraction, BusId};
pub use statement::PublicStatement;
