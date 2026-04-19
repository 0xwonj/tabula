// SP-5 §12 compile-fail probe: no `From<ProveError>` for `VerifyError`.
//
// The narrowed runtime error families must not convert into each other.
// Only `RuntimeError` is the common widening target. This fixture
// attempts a cross-narrow conversion and must not compile.

use tabula_runtime::{ProveError, VerifyError};

fn cross_narrow(_e: ProveError) -> VerifyError {
    _e.into()
}

fn main() {}
