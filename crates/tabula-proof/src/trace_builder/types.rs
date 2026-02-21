use std::collections::BTreeMap;

use p3_baby_bear::BabyBear;
use p3_matrix::dense::RowMajorMatrix;

use tabula_core::{ColId, TableId};

use crate::air::chips::inter_tx_order::trace::InterTxOrderRow;
use crate::air::chips::state_column::trace::StateColumnRow;

/// Output of the memory/metadata trace builder.
#[derive(Debug, Clone)]
pub struct ProofTraceBundle<const W: usize> {
    /// InterTxOrder rows (pre-trace representation).
    pub inter_tx_rows: Vec<InterTxOrderRow>,
    /// StateColumn rows (pre-trace representation).
    pub state_rows: Vec<StateColumnRow>,
    /// ColumnMeta empty-read multiplicity map.
    pub empty_read_mults: BTreeMap<(TableId, ColId), u32>,
    /// InterTxOrder chip trace.
    pub inter_tx_trace: RowMajorMatrix<BabyBear>,
    /// StateColumn chip trace.
    pub state_trace: RowMajorMatrix<BabyBear>,
    /// ColumnMeta chip trace.
    pub column_meta_trace: RowMajorMatrix<BabyBear>,
}

/// Full all-chip trace bundle for M12 orchestration.
#[derive(Debug, Clone)]
pub struct AllTraceBundle<const W: usize> {
    /// Memory/metadata traces.
    pub memory: ProofTraceBundle<W>,
    /// Execution trace.
    pub execution_trace: RowMajorMatrix<BabyBear>,
    /// StaticTable trace.
    pub static_table_trace: RowMajorMatrix<BabyBear>,
    /// SmtColPath trace.
    pub smt_col_path_trace: RowMajorMatrix<BabyBear>,
    /// SmtTablePath trace.
    pub smt_table_path_trace: RowMajorMatrix<BabyBear>,
    /// Poseidon trace synthesized from C5 sends.
    pub poseidon_trace: RowMajorMatrix<BabyBear>,
    /// Poseidon preprocessed trace.
    pub poseidon_preprocessed_trace: RowMajorMatrix<BabyBear>,
    /// RangeCheck trace synthesized from C8 sends.
    pub range_check_trace: RowMajorMatrix<BabyBear>,
}
