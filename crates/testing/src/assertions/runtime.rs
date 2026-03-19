use tabula_artifact::StateSnapshot;
use tabula_core::ExecutionConsistencyStatus;
use tabula_runtime::ExecutedBatch;

use crate::assertions::assert_state_snapshot_semantically_eq;

/// Assert that runtime consistency checks passed.
pub fn assert_runtime_consistency_passed(executed: &ExecutedBatch) {
    assert_eq!(
        executed.consistency,
        ExecutionConsistencyStatus::Passed,
        "runtime consistency should pass"
    );
}

/// Assert that the runtime post-state matches the expected snapshot.
pub fn assert_state_after_matches_expected(executed: &ExecutedBatch, expected: &StateSnapshot) {
    assert_state_snapshot_semantically_eq(&executed.state_after, expected);
}
