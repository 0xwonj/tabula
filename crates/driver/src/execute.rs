//! Canonical batch execution pipeline (delegated to [`tabula_runtime`]).
//!
//! The actual implementation lives in the `tabula-runtime` crate. This module
//! re-exports core types and provides a thin wrapper that converts
//! [`RuntimeError`](tabula_runtime::RuntimeError) to [`DriverError`] for
//! backward compatibility.

pub use tabula_runtime::{BatchInput, ExecutedBatch};

use crate::error::DriverError;

/// Execute a batch through the canonical pipeline.
///
/// Delegates to [`tabula_runtime::run_batch`] and maps runtime errors to
/// [`DriverError`] so existing consumers (CLI, daemon) continue to work
/// without changes.
pub fn run_batch(input: &BatchInput<'_>) -> Result<ExecutedBatch, DriverError> {
    tabula_runtime::run_batch(input).map_err(DriverError::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tabula_artifact::StateCell;
    use tabula_core::Value;
    use tabula_core::mock::Blake3Hasher;

    use crate::example::transfer_example_bundle;
    use crate::register::{MetadataPolicy, register_program_sources};

    #[test]
    fn run_batch_transfer_example() {
        let bundle = transfer_example_bundle().expect("example bundle");
        let registered =
            register_program_sources(&bundle.program, MetadataPolicy::Optional).expect("register");

        let executed = run_batch(&BatchInput {
            program: &registered.program,
            state: &bundle.state,
            batch: &bundle.batch,
            hasher: &Blake3Hasher,
        })
        .expect("run_batch");

        assert_eq!(executed.txs.len(), 3);
        assert!(executed.txs.iter().all(|tx| tx.is_success()));

        // After transfers: row0=1000-300+50=750, row1=500+300-200=600, row2=200+200-50=350
        let val = |row: u64| -> Option<Value> {
            executed
                .state_after
                .cells
                .iter()
                .find(|c| c.table == 0 && c.col == 0 && c.row == row)
                .and_then(|c| c.value)
        };
        assert_eq!(val(0), Some(Value::U64(750)));
        assert_eq!(val(1), Some(Value::U64(600)));
        assert_eq!(val(2), Some(Value::U64(350)));

        assert!(matches!(
            executed.consistency,
            tabula_core::ExecutionConsistencyStatus::Passed
        ));
    }

    #[test]
    fn run_batch_invalid_state() {
        let bundle = transfer_example_bundle().expect("example bundle");
        let registered =
            register_program_sources(&bundle.program, MetadataPolicy::Optional).expect("register");

        let bad_state = tabula_artifact::StateFile {
            cells: vec![StateCell {
                table: 0,
                row: 0,
                col: 0,
                value: None, // missing value
            }],
        };

        let err = run_batch(&BatchInput {
            program: &registered.program,
            state: &bad_state,
            batch: &bundle.batch,
            hasher: &Blake3Hasher,
        })
        .expect_err("invalid state should fail");
        assert!(matches!(err, DriverError::InvalidState(_)));
    }

    #[test]
    fn run_batch_empty_batch() {
        let bundle = transfer_example_bundle().expect("example bundle");
        let registered =
            register_program_sources(&bundle.program, MetadataPolicy::Optional).expect("register");

        let empty_batch = tabula_artifact::BatchFile {
            transactions: vec![],
        };

        let executed = run_batch(&BatchInput {
            program: &registered.program,
            state: &bundle.state,
            batch: &empty_batch,
            hasher: &Blake3Hasher,
        })
        .expect("run_batch");

        assert!(executed.txs.is_empty());
        assert!(executed.write_set.is_empty());
        // State passthrough: output equals normalized input
        assert_eq!(
            executed.state_after.cells.len(),
            executed.state_before.cells.len()
        );
    }
}
