//! Shared test-builder infrastructure for chip tests.
//!
//! Gated behind the `test-utils` feature flag.
//! Provides fluent builders and factory functions for constructing
//! test traces without per-test boilerplate.

pub mod builders;
pub mod values;
