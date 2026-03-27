//! Multi-chip integration tests: end-to-end LogUp bus verification.

use tabula_chips::execution::air::ExecutionChip;
use tabula_chips::execution::trace::generate_execution_trace;
use tabula_stark::debug::evaluate_chip;

use tabula_chips::test_utils::builders::make_read;

/// Smoke test: a single Read instruction passes constraint check.
#[test]
fn single_read_constraints_pass() {
    let records = vec![make_read(0, 1, 0, 100, 42, false)];
    let exec_trace = generate_execution_trace::<3>(&records);
    let exec_chip = ExecutionChip::<3>;
    evaluate_chip("Execution", &exec_chip, &exec_trace)
        .expect("single Read should pass all constraints");
}
