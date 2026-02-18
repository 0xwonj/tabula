//! AIR chip implementations.
//!
//! Each chip is a small struct implementing `BaseAir` + `Air`.
//! The `TabulaAir` enum dispatches to the appropriate chip.
//! All chips also implement `ChipMeta` for introspection.

pub mod column_meta;
pub mod execution;
pub mod merge;
pub mod poseidon;
pub mod range_check;
pub mod sorted_mem;
pub mod ssmc;

use p3_air::{Air, BaseAir};

use super::builder::InteractionAirBuilder;

use column_meta::ColumnMetaChip;
use execution::ExecutionChip;
use merge::GlobalMergeChip;
use poseidon::PoseidonChip;
use range_check::RangeCheckChip;
use sorted_mem::GlobalSortedMemChip;
use ssmc::GlobalSsmcChip;

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

impl<const W: usize> ChipMeta for GlobalSortedMemChip<W> {
    fn chip_name(&self) -> &'static str {
        "GlobalSortedMem"
    }
}

impl<const W: usize> ChipMeta for GlobalSsmcChip<W> {
    fn chip_name(&self) -> &'static str {
        "GlobalSSMC"
    }
}

impl<const W: usize> ChipMeta for GlobalMergeChip<W> {
    fn chip_name(&self) -> &'static str {
        "GlobalMerge"
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

/// Top-level AIR enum for multi-chip proving (M9).
///
/// Delegates `BaseAir`, `Air`, and `ChipMeta` to the contained chip variant.
#[derive(Debug)]
pub enum TabulaAir {
    /// ColumnMeta global table.
    ColumnMeta(ColumnMetaChip),
    /// Range check preprocessed table.
    RangeCheck(RangeCheckChip),
    /// GlobalSortedMem with Standard value width (W=3).
    SortedMemStandard(GlobalSortedMemChip<3>),
    /// GlobalSSMC with Standard value width (W=3).
    SsmcStandard(GlobalSsmcChip<3>),
    /// GlobalMerge with Standard value width (W=3).
    MergeStandard(GlobalMergeChip<3>),
    /// Poseidon2 permutation chip.
    Poseidon(PoseidonChip),
    /// Execution chip with Standard value width (W=3).
    ExecutionStandard(ExecutionChip<3>),
}

/// Dispatch macro: delegates a method call to all TabulaAir variants.
macro_rules! dispatch_tabula_air {
    ($self:ident, $method:ident $(, $arg:expr)*) => {
        match $self {
            Self::ColumnMeta(chip) => chip.$method($($arg),*),
            Self::RangeCheck(chip) => chip.$method($($arg),*),
            Self::SortedMemStandard(chip) => chip.$method($($arg),*),
            Self::SsmcStandard(chip) => chip.$method($($arg),*),
            Self::MergeStandard(chip) => chip.$method($($arg),*),
            Self::Poseidon(chip) => chip.$method($($arg),*),
            Self::ExecutionStandard(chip) => chip.$method($($arg),*),
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
            Self::SortedMemStandard(chip) => <GlobalSortedMemChip<3> as BaseAir<F>>::width(chip),
            Self::SsmcStandard(chip) => <GlobalSsmcChip<3> as BaseAir<F>>::width(chip),
            Self::MergeStandard(chip) => <GlobalMergeChip<3> as BaseAir<F>>::width(chip),
            Self::Poseidon(chip) => <PoseidonChip as BaseAir<F>>::width(chip),
            Self::ExecutionStandard(chip) => <ExecutionChip<3> as BaseAir<F>>::width(chip),
        }
    }
}

impl<AB: InteractionAirBuilder> Air<AB> for TabulaAir {
    fn eval(&self, builder: &mut AB) {
        dispatch_tabula_air!(self, eval, builder)
    }
}
