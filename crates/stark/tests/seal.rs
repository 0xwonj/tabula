//! Trybuild probes that enforce the `ChipWitnessKit` convention seal.
//!
//! Two probes per SP-5 §9:
//! - `compile_fail`: implementing `ChipWitnessKit` without `sealed::Sealed`
//!   must not compile.
//! - `pass`: implementing `ChipWitnessKit` *with* `sealed::Sealed` must
//!   compile. This guards against a trivially-unreachable seal (where the
//!   test would "pass" because `Sealed` itself cannot be named).

#[test]
fn chip_witness_kit_is_sealed() {
    let t = trybuild::TestCases::new();
    t.compile_fail("tests/ui/external_impl_no_sealed.rs");
    t.pass("tests/ui/external_impl_with_sealed.rs");
}
