//! Declarative chip set composition via the [`define_chip_set!`] macro.
//!
//! External zkVM projects can define custom chip sets by invoking the macro
//! with their own enum name and chip variants.

use p3_air::BaseAir;
use p3_baby_bear::BabyBear;

use crate::air::interaction::BusId;
use crate::chips::{ChipId, ChipSpec};

/// A chip set: compile-time composition of AIR chips.
///
/// Provides chip enumeration and [`ChipId`]-based lookup for the prover/verifier.
/// Includes `BaseAir<BabyBear>` so generic prover/verifier code can query widths.
pub trait ChipSet: ChipSpec + BaseAir<BabyBear> + Sized + std::fmt::Debug {
    /// Instantiate all chip variants (one per variant).
    fn all_chips() -> Vec<Self>;
    /// Reconstruct a chip from its [`ChipId`] (for the verifier).
    fn from_id(id: ChipId) -> Option<Self>;
    /// List all chip IDs in declaration order.
    fn chip_ids() -> Vec<ChipId>;
    /// All bus IDs participating in this chip set (union of all chips' bus usage).
    ///
    /// Used by the generic validation pipeline for bus balance checks.
    /// Defaults to [`core_buses::ALL`](crate::air::interaction::core_buses::ALL).
    fn bus_manifest() -> Vec<BusId> {
        crate::air::interaction::core_buses::ALL.to_vec()
    }
}

/// Define a chip set enum with automatic trait dispatch.
///
/// Generates:
/// - The enum itself with `Debug` + `Default` (defaults to the first variant)
/// - `ChipSpec` dispatch (`chip_id`, `num_public_values`, `preprocessed_width`, `has_interactions`)
/// - `BaseAir<F>` dispatch (`width`)
/// - `BaseAirWithPublicValues<F>` dispatch (`num_public_values`)
/// - `Air<AB>` dispatch where `AB: InteractionAirBuilder + AirBuilderWithPublicValues`
/// - `ChipSet` impl (`all_chips`, `from_id`, `chip_ids`)
///
/// # Example
///
/// ```ignore
/// define_chip_set! {
///     pub enum TabulaAir {
///         Execution(ExecutionChip<3>),
///         ColumnMeta(ColumnMetaChip),
///         Poseidon(PoseidonChip),
///         RangeCheck(RangeCheckChip),
///     }
/// }
/// ```
#[macro_export]
macro_rules! define_chip_set {
    (
        $(#[$meta:meta])*
        $vis:vis enum $name:ident {
            $(
                $(#[$vmeta:meta])*
                $variant:ident ( $chip:ty )
            ),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Debug)]
        $vis enum $name {
            $(
                $(#[$vmeta])*
                $variant($chip),
            )+
        }

        // Default: first variant.
        impl Default for $name {
            fn default() -> Self {
                $crate::define_chip_set!(@first_variant $name, $($variant($chip)),+)
            }
        }

        impl $crate::chips::ChipSpec for $name {
            fn chip_id(&self) -> $crate::chips::ChipId {
                match self {
                    $(Self::$variant(chip) => $crate::chips::ChipSpec::chip_id(chip),)+
                }
            }
            fn num_public_values(&self) -> usize {
                match self {
                    $(Self::$variant(chip) => $crate::chips::ChipSpec::num_public_values(chip),)+
                }
            }
            fn preprocessed_width(&self) -> usize {
                match self {
                    $(Self::$variant(chip) => $crate::chips::ChipSpec::preprocessed_width(chip),)+
                }
            }
            fn has_interactions(&self) -> bool {
                match self {
                    $(Self::$variant(chip) => $crate::chips::ChipSpec::has_interactions(chip),)+
                }
            }
        }

        impl<F> p3_air::BaseAir<F> for $name {
            fn width(&self) -> usize {
                match self {
                    $(Self::$variant(chip) => <$chip as p3_air::BaseAir<F>>::width(chip),)+
                }
            }
        }

        impl<F> p3_air::BaseAirWithPublicValues<F> for $name {
            fn num_public_values(&self) -> usize {
                $crate::chips::ChipSpec::num_public_values(self)
            }
        }

        impl<AB> p3_air::Air<AB> for $name
        where
            AB: $crate::air::builder::InteractionAirBuilder
                + p3_air::AirBuilderWithPublicValues,
        {
            fn eval(&self, builder: &mut AB) {
                match self {
                    $(Self::$variant(chip) => <$chip as p3_air::Air<AB>>::eval(chip, builder),)+
                }
            }
        }

        impl $crate::trace::contributor::TraceContributor for $name {
            fn phase(&self) -> $crate::trace::contributor::TracePhase {
                match self {
                    $(Self::$variant(chip) => {
                        $crate::trace::contributor::TraceContributor::phase(chip)
                    },)+
                }
            }

            fn contribute(
                &self,
                store: &$crate::trace::contributor::WitnessStore,
                map: &mut $crate::trace::trace_map::TraceMap,
            ) -> Result<(), tabula_core::error::TabulaError> {
                match self {
                    $(Self::$variant(chip) => {
                        $crate::trace::contributor::TraceContributor::contribute(chip, store, map)
                    },)+
                }
            }
        }

        impl $crate::air::chip_set::ChipSet for $name {
            fn all_chips() -> Vec<Self> {
                vec![
                    $(Self::$variant(<$chip as Default>::default()),)+
                ]
            }

            fn from_id(id: $crate::chips::ChipId) -> Option<Self> {
                $(
                    if id == $crate::chips::ChipSpec::chip_id(
                        &<$chip as Default>::default()
                    ) {
                        return Some(Self::$variant(<$chip as Default>::default()));
                    }
                )+
                None
            }

            fn chip_ids() -> Vec<$crate::chips::ChipId> {
                vec![
                    $($crate::chips::ChipSpec::chip_id(
                        &<$chip as Default>::default()
                    ),)+
                ]
            }
        }
    };

    // Helper: extract the first variant from the list.
    (@first_variant $name:ident, $first_variant:ident($first_chip:ty) $(, $rest:ident($rchip:ty))*) => {
        $name::$first_variant(<$first_chip as Default>::default())
    };
}
