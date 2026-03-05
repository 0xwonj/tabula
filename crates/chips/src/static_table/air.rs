//! StaticTableChip — AIR constraints for static table lookups.
//!
//! Constraints:
//! 1. `is_real` boolean + prefix (monotonic 1→0)
//! 2. C9 StaticTableLookup receive with multiplicity witness

use p3_air::{Air, BaseAir};
use p3_matrix::Matrix;

use tabula_gadgets::constrain_is_real_prefix;
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::bus::StaticTableLookupAirBuilder;
use tabula_stark::air::columns::borrow_cols;

use super::columns::{StaticTableCols, static_table_width};

/// The StaticTable AIR chip.
#[derive(Debug)]
pub struct StaticTableChip<const W: usize>;

impl<F, const W: usize> BaseAir<F> for StaticTableChip<W> {
    fn width(&self) -> usize {
        static_table_width::<W>()
    }
}

impl<AB: InteractionAirBuilder, const W: usize> Air<AB> for StaticTableChip<W> {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local_row = main.row_slice(0).expect("trace must have at least one row");
        let next_row = main
            .row_slice(1)
            .expect("trace must have at least two rows");
        let local: &StaticTableCols<AB::Var, W> = borrow_cols(&local_row);
        let next: &StaticTableCols<AB::Var, W> = borrow_cols(&next_row);

        // ── 1. is_real prefix ──
        constrain_is_real_prefix(builder, local.is_real.clone(), next.is_real.clone());

        // ── 2. C9 StaticTableLookup receive ──
        builder.receive_static_table_lookup(
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            &local.row_key,
            &local.value,
            local.is_real.clone().into() * local.lookup_mult_witness.clone().into(),
        );
    }
}
