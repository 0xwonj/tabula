//! Fixed execution-tier composition for the Tabula machine.

use tabula_chips::execution::ExecutionChip;
use tabula_chips::static_table::StaticTableChip;
use tabula_stark::chips::DEFAULT_VALUE_WIDTH;
use tabula_stark::trace::DynChip;

use crate::AnyRap;

/// Fixed execution-layer AIRs for proving/verifying.
pub(crate) fn execution_airs() -> Vec<Box<dyn AnyRap>> {
    vec![
        Box::new(ExecutionChip::<DEFAULT_VALUE_WIDTH>),
        Box::new(StaticTableChip::<DEFAULT_VALUE_WIDTH>),
    ]
}

/// Fixed execution-layer chips for trace building and debug validation.
pub(crate) fn execution_dyn_chips() -> Vec<Box<dyn DynChip>> {
    vec![
        Box::new(ExecutionChip::<DEFAULT_VALUE_WIDTH>),
        Box::new(StaticTableChip::<DEFAULT_VALUE_WIDTH>),
    ]
}
