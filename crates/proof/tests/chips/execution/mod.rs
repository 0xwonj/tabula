use p3_baby_bear::BabyBear;
use p3_field::PrimeCharacteristicRing;

use tabula_proof::air::borrow_cols_mut;
use tabula_proof::chips::execution::air::ExecutionChip;
use tabula_proof::chips::execution::columns::{
    EXECUTION_STANDARD_WIDTH, ExecutionCols, execution_width,
};
use tabula_proof::chips::execution::trace::{CmpOp, InstructionRecord, generate_execution_trace};
use tabula_proof::chips::execution::trace_utils::{limbs_to_u64, u64_to_limbs};
use tabula_proof::debug::debug_check;

use crate::common::builders::{
    make_add, make_and, make_assert, make_cmp, make_divmod, make_hash, make_lookup, make_mul,
    make_not, make_or, make_read, make_read_then_add, make_select, make_sub, make_write,
};

const W: usize = 3;

mod basic;
mod ops;
mod regression;
