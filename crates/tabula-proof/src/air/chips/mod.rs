//! AIR chip implementations.
//!
//! Each chip is a small struct implementing `BaseAir` + `Air`.
//! The `TabulaAir` enum dispatches to the appropriate chip.
//! All chips also implement `ChipMeta` for introspection.

pub mod column_meta;
pub mod execution;
pub mod inter_tx_order;
pub mod poseidon;
pub mod range_check;
pub mod smt_path;
pub mod state_column;
pub mod static_table;

use p3_air::{Air, AirBuilderWithPublicValues, BaseAir};

use super::builder::InteractionAirBuilder;

use column_meta::ColumnMetaChip;
use execution::ExecutionChip;
use inter_tx_order::InterTxOrderChip;
use poseidon::PoseidonChip;
use range_check::RangeCheckChip;
use smt_path::{SmtColPathChip, SmtTablePathChip};
use state_column::StateColumnChip;
use static_table::StaticTableChip;

/// Metadata interface for AIR chips.
///
/// Object-safe trait for introspection. Chip name is used by debug
/// checking and error messages.
///
/// LogUp interactions are declared via [`super::builder::InteractionAirBuilder`]
/// during `eval()`, not via this trait.
pub trait ChipMeta {
    /// Human-readable chip name (e.g. `"ColumnMeta"`).
    fn chip_name(&self) -> &'static str;
}

impl ChipMeta for ColumnMetaChip {
    fn chip_name(&self) -> &'static str {
        "ColumnMeta"
    }
}

impl ChipMeta for RangeCheckChip {
    fn chip_name(&self) -> &'static str {
        "RangeCheck"
    }
}

impl ChipMeta for PoseidonChip {
    fn chip_name(&self) -> &'static str {
        "Poseidon"
    }
}

impl<const W: usize> ChipMeta for ExecutionChip<W> {
    fn chip_name(&self) -> &'static str {
        "Execution"
    }
}

impl<const W: usize> ChipMeta for StateColumnChip<W> {
    fn chip_name(&self) -> &'static str {
        "StateColumn"
    }
}

impl<const W: usize> ChipMeta for InterTxOrderChip<W> {
    fn chip_name(&self) -> &'static str {
        "InterTxOrder"
    }
}

impl<const W: usize> ChipMeta for StaticTableChip<W> {
    fn chip_name(&self) -> &'static str {
        "StaticTable"
    }
}

impl ChipMeta for SmtColPathChip {
    fn chip_name(&self) -> &'static str {
        "SmtColPath"
    }
}

impl ChipMeta for SmtTablePathChip {
    fn chip_name(&self) -> &'static str {
        "SmtTablePath"
    }
}

/// Top-level AIR enum for multi-chip proving.
///
/// Delegates `BaseAir`, `Air`, and `ChipMeta` to the contained chip variant.
#[derive(Debug)]
pub enum TabulaAir {
    /// ColumnMeta global table.
    ColumnMeta(ColumnMetaChip),
    /// Range check preprocessed table.
    RangeCheck(RangeCheckChip),
    /// Poseidon2 permutation chip.
    Poseidon(PoseidonChip),
    /// Execution chip with Standard value width (W=3).
    ExecutionStandard(ExecutionChip<3>),
    /// StateColumn chip with Standard value width (W=3).
    StateColumnStandard(StateColumnChip<3>),
    /// InterTxOrder chip with Standard value width (W=3).
    InterTxOrderStandard(InterTxOrderChip<3>),
    /// StaticTable chip with Standard value width (W=3).
    StaticTableStandard(StaticTableChip<3>),
    /// SmtColPath chip (column-level SMT paths).
    SmtColPath(SmtColPathChip),
    /// SmtTablePath chip (table-level SMT paths).
    SmtTablePath(SmtTablePathChip),
}

/// Dispatch macro: delegates a method call to all TabulaAir variants.
macro_rules! dispatch_tabula_air {
    ($self:ident, $method:ident $(, $arg:expr)*) => {
        match $self {
            Self::ColumnMeta(chip) => chip.$method($($arg),*),
            Self::RangeCheck(chip) => chip.$method($($arg),*),
            Self::Poseidon(chip) => chip.$method($($arg),*),
            Self::ExecutionStandard(chip) => chip.$method($($arg),*),
            Self::StateColumnStandard(chip) => chip.$method($($arg),*),
            Self::InterTxOrderStandard(chip) => chip.$method($($arg),*),
            Self::StaticTableStandard(chip) => chip.$method($($arg),*),
            Self::SmtColPath(chip) => chip.$method($($arg),*),
            Self::SmtTablePath(chip) => chip.$method($($arg),*),
        }
    };
}

impl ChipMeta for TabulaAir {
    fn chip_name(&self) -> &'static str {
        dispatch_tabula_air!(self, chip_name)
    }
}

impl<F> BaseAir<F> for TabulaAir {
    fn width(&self) -> usize {
        match self {
            Self::ColumnMeta(chip) => <ColumnMetaChip as BaseAir<F>>::width(chip),
            Self::RangeCheck(chip) => <RangeCheckChip as BaseAir<F>>::width(chip),
            Self::Poseidon(chip) => <PoseidonChip as BaseAir<F>>::width(chip),
            Self::ExecutionStandard(chip) => <ExecutionChip<3> as BaseAir<F>>::width(chip),
            Self::StateColumnStandard(chip) => <StateColumnChip<3> as BaseAir<F>>::width(chip),
            Self::InterTxOrderStandard(chip) => <InterTxOrderChip<3> as BaseAir<F>>::width(chip),
            Self::StaticTableStandard(chip) => <StaticTableChip<3> as BaseAir<F>>::width(chip),
            Self::SmtColPath(chip) => <SmtColPathChip as BaseAir<F>>::width(chip),
            Self::SmtTablePath(chip) => <SmtTablePathChip as BaseAir<F>>::width(chip),
        }
    }
}

impl<AB> Air<AB> for TabulaAir
where
    AB: InteractionAirBuilder + AirBuilderWithPublicValues,
{
    fn eval(&self, builder: &mut AB) {
        dispatch_tabula_air!(self, eval, builder)
    }
}
