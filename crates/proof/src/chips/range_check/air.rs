//! RangeCheck AIR chip — no constraints, soundness via LogUp.

use p3_air::{Air, BaseAir};
use p3_matrix::Matrix;

use crate::air::builder::InteractionAirBuilder;
use crate::air::columns::borrow_cols;
use crate::air::interaction::{AirInteraction, InteractionKind};

use super::columns::{RANGE_CHECK_WIDTH, RangeCheckCols};

/// The RangeCheck AIR chip.
///
/// No constraints — the table is preprocessed. Soundness comes from the LogUp
/// argument: any chip sending a range-check request must have a matching entry
/// in this table. If the requested value is outside `[0, 2^16)`, no matching
/// row exists and the LogUp argument fails.
#[derive(Debug)]
pub struct RangeCheckChip;

impl<F> BaseAir<F> for RangeCheckChip {
    fn width(&self) -> usize {
        RANGE_CHECK_WIDTH
    }
}

impl<AB: InteractionAirBuilder> Air<AB> for RangeCheckChip {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local_row = main.row_slice(0).expect("trace must have at least one row");
        let local: &RangeCheckCols<AB::Var> = borrow_cols(&local_row);

        // C8 RangeCheck bus receive: one receive per row.
        // value is fixed (0..2^16), multiplicity is prover-filled.
        builder.receive(AirInteraction {
            values: vec![local.value.clone().into()],
            multiplicity: local.multiplicity.clone().into(),
            kind: InteractionKind::RangeCheck,
        });
    }
}
