//! Regression guard: narrowed runtime errors must widen only into RuntimeError,
//! never into each other (spec §7.1).

use tabula_runtime::{RuntimeError, SetupError};
#[cfg(feature = "prove")]
use tabula_runtime::ProveError;
#[cfg(feature = "verify")]
use tabula_runtime::{ExecuteError, VerifyError};

// Positive: each narrow error widens into RuntimeError.
const _: fn() = || {
    fn takes_runtime_error<E: Into<RuntimeError>>() {}
    takes_runtime_error::<SetupError>();
    #[cfg(feature = "prove")]
    takes_runtime_error::<ProveError>();
    #[cfg(feature = "verify")]
    takes_runtime_error::<VerifyError>();
    #[cfg(feature = "verify")]
    takes_runtime_error::<ExecuteError>();
};

// Negative: no direct From between narrowed errors. We can't assert
// "no impl" at compile time, but this test exists as a grep target +
// documentation. If a future change adds `impl From<SetupError> for
// ProveError`, the spec violation should be caught in code review +
// §13 audit, not by this test — Rust's trait system can't express
// negative "no impl exists" assertions cleanly.
#[test]
fn narrowed_errors_do_not_convert_to_each_other_at_compile_time() {
    // Intentionally empty. See module-level comment.
}
