//! PropertyVerifierChip -- AIR constraints for property query verification.
//!
//! Receives from the `PROPERTY_READ` external bus (BusId 18), verifying
//! that property query results from the execution tier are consistent.
//!
//! Constraint groups:
//! 1. Boolean: is_real, is_null
//! 2. is_real prefix: monotonic 1->0
//! 3. Constant identity: table_id, col_id same across all real rows
//! 4. LogUp: PROPERTY_READ bus receive

use p3_air::{Air, AirBuilder, BaseAir, WindowAccess};

use tabula_gadgets::constrain_is_real_prefix;
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::bus::PropertyReadAirBuilder;
use tabula_stark::air::columns::borrow_cols;
use tabula_stark::chips::ChipId;

use crate::ChipSpec;

use super::columns::{PropertyVerifierCols, property_verifier_width};

/// Per-column property verifier AIR chip.
///
/// Each instance operates on a single `(table_id, col_id)` pair.
/// Receives from the PROPERTY_READ external bus to absorb cross-tier
/// query results from the execution tier.
#[derive(Debug, Clone)]
pub struct PropertyVerifierChip<const W: usize> {
    chip_id: ChipId,
    table_id: u32,
    col_id: u16,
}

impl<const W: usize> PropertyVerifierChip<W> {
    /// Create a new property verifier chip for a specific column.
    pub fn new(chip_id: ChipId, table_id: u32, col_id: u16) -> Self {
        Self {
            chip_id,
            table_id,
            col_id,
        }
    }

    /// Table identifier this chip verifies.
    pub fn table_id(&self) -> u32 {
        self.table_id
    }

    /// Column identifier this chip verifies.
    pub fn col_id(&self) -> u16 {
        self.col_id
    }
}

impl<const W: usize> ChipSpec for PropertyVerifierChip<W> {
    fn chip_id(&self) -> ChipId {
        self.chip_id
    }

    fn chip_name(&self) -> &'static str {
        "PropertyVerifier"
    }
}

impl<F, const W: usize> BaseAir<F> for PropertyVerifierChip<W> {
    fn width(&self) -> usize {
        property_verifier_width::<W>()
    }
}

impl<AB: InteractionAirBuilder, const W: usize> Air<AB> for PropertyVerifierChip<W> {
    fn eval(&self, builder: &mut AB) {
        let main = builder.main();
        let local_row = main.current_slice();
        let next_row = main.next_slice();
        let local: &PropertyVerifierCols<AB::Var, W> = borrow_cols(local_row);
        let next: &PropertyVerifierCols<AB::Var, W> = borrow_cols(next_row);

        let is_real: AB::Expr = local.is_real.clone().into();

        // 1. Boolean constraints
        builder.assert_bool(local.is_real.clone());
        builder.assert_bool(local.is_null.clone());

        // 2. is_real prefix (monotonic 1->0)
        constrain_is_real_prefix(builder, local.is_real.clone(), next.is_real.clone());

        // 3. Constant identity: table_id, col_id same across real rows
        let both_real: AB::Expr = is_real.clone() * next.is_real.clone().into();
        builder.when_transition().assert_zero(
            both_real.clone() * (next.table_id.clone().into() - local.table_id.clone().into()),
        );
        builder
            .when_transition()
            .assert_zero(both_real * (next.col_id.clone().into() - local.col_id.clone().into()));

        // 4. Receive from PROPERTY_READ bus
        builder.receive_property_read(
            local.table_id.clone().into(),
            local.col_id.clone().into(),
            local.query_type.clone().into(),
            &local.result_val,
            &local.result_key,
            local.is_null.clone().into(),
            is_real, // multiplicity
        );
    }
}
