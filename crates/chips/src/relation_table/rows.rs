//! Dedicated proof lane for static canonical relation tables.
//!
//! The witness rows are the sealed relation rows derived from the registered
//! program. AIR binds execution membership sends to those rows and binds the
//! full static table to one public root.
#![allow(unused_imports)]

use p3_air::{Air, AirBuilder, BaseAir, WindowAccess};
use p3_field::{PrimeCharacteristicRing, PrimeField32};
use p3_koala_bear::KoalaBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_gadgets::constrain_is_real_prefix;
use tabula_gadgets::integer::expr_from_u32;
use tabula_stark::air::builder::InteractionAirBuilder;
use tabula_stark::air::columns::{borrow_cols, borrow_cols_mut, num_cols};
use tabula_stark::air::interaction::{AirInteraction, BusId};
use tabula_stark::chips::{ChipId, ChipSpec};
use tabula_stark::trace::TraceGenerator;
use tabula_stark::trace::contributor::{TraceContributor, TracePhase, WitnessStore};
use tabula_stark::trace::trace_map::TraceMap;

use tabula_core::error::TabulaError;

use crate::poseidon::air as poseidon_air;
use crate::poseidon::columns::{POSEIDON_PREPROCESSED_WIDTH, PoseidonCols};
use crate::poseidon::constants::{TOTAL_ROUNDS, WIDTH, is_full_round, poseidon2_permutation};
use crate::poseidon::generate_poseidon_preprocessed;

/// Witness-store label consumed by [`RelationTableChip`].
pub const RELATION_TABLE_WITNESS_LABEL: &str = "relation_table_rows";
/// Chip id for the static relation lookup lane.
pub const RELATION_TABLE_CHIP_ID: ChipId = ChipId(92);
/// Private bus carrying `(relation_id, input_digest, output_digest)` tuples.
pub const RELATION_TABLE_BUS: BusId = BusId(101);

pub(super) const RELATION_TABLE_DOMAIN_TAG: u32 = 0x52;

/// One static relation lookup row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RelationTableWitnessRow {
    /// Relation identifier.
    pub relation_id: u32,
    /// Canonical input digest.
    pub input_digest: [u32; 8],
    /// Canonical output digest.
    pub output_digest: [u32; 8],
    /// Multiplicity on the lookup bus.
    pub lookup_mult: u32,
}
