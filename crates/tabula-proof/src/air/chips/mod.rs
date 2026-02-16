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

use p3_air::{Air, AirBuilder, BaseAir};

use column_meta::ColumnMetaChip;
use execution::ExecutionChip;
use merge::GlobalMergeChip;
use poseidon::PoseidonChip;
use range_check::RangeCheckChip;
use sorted_mem::GlobalSortedMemChip;
use ssmc::GlobalSsmcChip;

use super::bus::InteractionDecl;

/// Metadata interface for AIR chips.
///
/// Object-safe trait for introspection: chip name and declared interactions.
/// Used by future dynamic chip dispatch and debugging infrastructure.
pub trait ChipMeta {
    /// Human-readable chip name (e.g. `"ColumnMeta"`).
    fn chip_name(&self) -> &'static str;

    /// LogUp interaction declarations for this chip.
    ///
    /// Empty until interactions are wired in M9.
    fn interactions(&self) -> Vec<InteractionDecl>;
}

impl ChipMeta for ColumnMetaChip {
    fn chip_name(&self) -> &'static str {
        "ColumnMeta"
    }

    fn interactions(&self) -> Vec<InteractionDecl> {
        vec![]
    }
}

impl ChipMeta for RangeCheckChip {
    fn chip_name(&self) -> &'static str {
        "RangeCheck"
    }

    fn interactions(&self) -> Vec<InteractionDecl> {
        // Wired in M9: RangeCheck receive.
        vec![]
    }
}

impl<const W: usize> ChipMeta for GlobalSortedMemChip<W> {
    fn chip_name(&self) -> &'static str {
        "GlobalSortedMem"
    }

    fn interactions(&self) -> Vec<InteractionDecl> {
        // Wired in M9: Memory receive, SsmcMembership send (init rows),
        // MergeCompleteness send (write-set), ColumnMetaJoin send.
        vec![]
    }
}

impl<const W: usize> ChipMeta for GlobalSsmcChip<W> {
    fn chip_name(&self) -> &'static str {
        "GlobalSSMC"
    }

    fn interactions(&self) -> Vec<InteractionDecl> {
        // Wired in M9: SsmcMembership receive, MergeCompleteness OldList send,
        // ColumnMetaJoin send, hash chain via PoseidonPermutation.
        vec![]
    }
}

impl<const W: usize> ChipMeta for GlobalMergeChip<W> {
    fn chip_name(&self) -> &'static str {
        "GlobalMerge"
    }

    fn interactions(&self) -> Vec<InteractionDecl> {
        // Wired in M9: MergeCompleteness receive (OldList + WriteSet sub-buses),
        // ColumnMetaJoin send, hash chain via PoseidonPermutation.
        vec![]
    }
}

impl ChipMeta for PoseidonChip {
    fn chip_name(&self) -> &'static str {
        "Poseidon"
    }

    fn interactions(&self) -> Vec<InteractionDecl> {
        // Wired in M9: PoseidonPermutation bus, receive from SSMC/Merge/Hash.
        vec![]
    }
}

impl<const W: usize> ChipMeta for ExecutionChip<W> {
    fn chip_name(&self) -> &'static str {
        "Execution"
    }

    fn interactions(&self) -> Vec<InteractionDecl> {
        // Wired in M9: Memory bus send (is_access rows),
        // RangeCheck send (limb ranges), operand-slot linkage.
        vec![]
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

    fn interactions(&self) -> Vec<InteractionDecl> {
        dispatch_tabula_air!(self, interactions)
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

impl<AB: AirBuilder> Air<AB> for TabulaAir {
    fn eval(&self, builder: &mut AB) {
        dispatch_tabula_air!(self, eval, builder)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chip_meta_name() {
        let chip = ColumnMetaChip;
        assert_eq!(chip.chip_name(), "ColumnMeta");
    }

    #[test]
    fn chip_meta_interactions_empty() {
        let chip = ColumnMetaChip;
        assert!(chip.interactions().is_empty());
    }

    #[test]
    fn tabula_air_delegates_chip_meta() {
        let air = TabulaAir::ColumnMeta(ColumnMetaChip);
        assert_eq!(air.chip_name(), "ColumnMeta");
        assert!(air.interactions().is_empty());
    }

    #[test]
    fn tabula_air_range_check() {
        let air = TabulaAir::RangeCheck(RangeCheckChip);
        assert_eq!(air.chip_name(), "RangeCheck");
        assert!(air.interactions().is_empty());
    }

    #[test]
    fn tabula_air_ssmc() {
        let air = TabulaAir::SsmcStandard(GlobalSsmcChip::<3>);
        assert_eq!(air.chip_name(), "GlobalSSMC");
        assert!(air.interactions().is_empty());
    }

    #[test]
    fn tabula_air_merge() {
        let air = TabulaAir::MergeStandard(GlobalMergeChip::<3>);
        assert_eq!(air.chip_name(), "GlobalMerge");
        assert!(air.interactions().is_empty());
    }

    #[test]
    fn tabula_air_poseidon() {
        let air = TabulaAir::Poseidon(PoseidonChip);
        assert_eq!(air.chip_name(), "Poseidon");
        assert!(air.interactions().is_empty());
    }

    #[test]
    fn tabula_air_execution() {
        let air = TabulaAir::ExecutionStandard(ExecutionChip::<3>);
        assert_eq!(air.chip_name(), "Execution");
        assert!(air.interactions().is_empty());
    }
}
