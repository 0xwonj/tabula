//! Regression guard: narrowed runtime errors must widen only into RuntimeError,
//! never into each other (spec §7.1).
//!
//! SP-5 §12: the trybuild probe below enforces this at the type-system level
//! via a compile-fail fixture.

#[cfg(feature = "prove")]
use tabula_runtime::ProveError;
#[cfg(feature = "verify")]
use tabula_runtime::{ExecuteError, VerifyError};
use tabula_runtime::{RuntimeError, SetupError};

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

#[test]
fn narrowed_errors_do_not_convert_to_each_other_at_compile_time() {
    // Intentionally empty. See module-level comment.
}

// Negative: compile-fail probe asserts that a cross-narrow `From<ProveError>`
// for `VerifyError` does not exist. The trybuild fixture in
// `tests/ui/error_conversions/` is gated on `prove` + `verify` because both
// error types are feature-gated.
#[cfg(all(feature = "prove", feature = "verify"))]
#[test]
fn no_from_between_narrowed_error_families() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/error_conversions/no_from_prove_to_verify.rs");
}
