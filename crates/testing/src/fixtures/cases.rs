//! Canonical scenario fixtures.

use tabula_artifact::{Artifact, State, TransactionBatch};
use tabula_compiler::SealedProgram;
use tabula_core::{ColId, RowKey, TableId, Transaction, Value};

use crate::fixtures::batch::{core_tx, single_tx_batch};
use crate::fixtures::programs::{
    arith_add_sub_source, cmp_assert_source, liquid_shielded_bump_source, peek_accounts_source,
    shielded_peek_source, touch_accounts_source, transfer_balances_source,
};
use crate::fixtures::state::{liquid_shielded_state, single_cell_u64};

#[derive(Clone, Debug)]
pub struct TraceCase {
    pub source: &'static str,
    pub initial_cells: Vec<(TableId, ColId, RowKey, Value)>,
    pub transactions: Vec<Transaction>,
}

#[derive(Clone, Debug)]
pub struct RuntimeCase {
    pub source: &'static str,
    pub state: State,
    pub batch: TransactionBatch,
}

#[derive(Clone, Debug)]
pub struct CompiledRuntimeCase {
    pub compiled_program: SealedProgram,
    pub state: State,
    pub batch: TransactionBatch,
}

#[derive(Clone, Debug)]
pub struct ArtifactRuntimeCase {
    pub artifact: Artifact,
    pub state: State,
    pub batch: TransactionBatch,
}

pub fn touch_trace_case() -> TraceCase {
    TraceCase {
        source: touch_accounts_source(),
        initial_cells: vec![(TableId(0), ColId(0), RowKey(10), Value::U64(50))],
        transactions: vec![core_tx(0, vec![Value::U64(10)], 0)],
    }
}

pub fn arith_add_sub_trace_case() -> TraceCase {
    TraceCase {
        source: arith_add_sub_source(),
        initial_cells: vec![(TableId(0), ColId(0), RowKey(10), Value::U64(100))],
        transactions: vec![core_tx(0, vec![Value::U64(10)], 0)],
    }
}

pub fn cmp_assert_trace_case() -> TraceCase {
    TraceCase {
        source: cmp_assert_source(),
        initial_cells: vec![(TableId(0), ColId(0), RowKey(5), Value::U64(100))],
        transactions: vec![core_tx(0, vec![Value::U64(5)], 0)],
    }
}

pub fn single_transfer_trace_case() -> TraceCase {
    TraceCase {
        source: transfer_balances_source(),
        initial_cells: vec![
            (TableId(0), ColId(0), RowKey(0), Value::U64(1000)),
            (TableId(0), ColId(0), RowKey(1), Value::U64(500)),
        ],
        transactions: vec![core_tx(
            0,
            vec![Value::U64(0), Value::U64(1), Value::U64(300)],
            0,
        )],
    }
}

pub fn mixed_outcome_transfer_trace_case() -> TraceCase {
    TraceCase {
        source: transfer_balances_source(),
        initial_cells: vec![
            (TableId(0), ColId(0), RowKey(0), Value::U64(1000)),
            (TableId(0), ColId(0), RowKey(1), Value::U64(500)),
            (TableId(0), ColId(0), RowKey(2), Value::U64(200)),
        ],
        transactions: vec![
            core_tx(0, vec![Value::U64(0), Value::U64(1), Value::U64(300)], 0),
            core_tx(0, vec![Value::U64(0), Value::U64(2), Value::U64(800)], 1),
            core_tx(0, vec![Value::U64(1), Value::U64(2), Value::U64(100)], 1),
        ],
    }
}

pub fn peek_runtime_case() -> RuntimeCase {
    RuntimeCase {
        source: peek_accounts_source(),
        state: single_cell_u64(TableId(0), ColId(0), RowKey(0), 10),
        batch: single_tx_batch(0, vec![]),
    }
}

pub fn shielded_peek_runtime_case() -> RuntimeCase {
    RuntimeCase {
        source: shielded_peek_source(),
        state: single_cell_u64(TableId(0), ColId(0), RowKey(0), 20),
        batch: single_tx_batch(0, vec![]),
    }
}

pub fn liquid_shielded_bump_runtime_case() -> RuntimeCase {
    RuntimeCase {
        source: liquid_shielded_bump_source(),
        state: liquid_shielded_state(10, 20),
        batch: single_tx_batch(0, vec![Value::U64(5)]),
    }
}
