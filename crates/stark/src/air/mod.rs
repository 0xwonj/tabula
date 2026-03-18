//! AIR constraint framework for the Tabula proof system.
//!
//! Provides column struct utilities, cross-chip interaction types,
//! interaction builder trait, and chip set composition macro.

pub mod builder;
#[macro_use]
pub mod bus_macro;
pub mod bus;
pub mod columns;
pub mod descriptor;
pub mod interaction;
pub mod keygen;
pub mod primitives;
pub mod statement;

pub use builder::InteractionAirBuilder;
pub use columns::{borrow_cols, borrow_cols_mut, num_cols};
pub use interaction::{AirInteraction, BusId};
pub use statement::PublicStatement;
