//! Tests for the AnyRap trait: object safety, blanket impl coverage, vtable dispatch.

use p3_air::{Air, BaseAir};
use p3_baby_bear::BabyBear;
use p3_uni_stark::SymbolicAirBuilder;

use tabula_chips::column_meta::ColumnMetaChip;
use tabula_chips::execution::ExecutionChip;
use tabula_chips::inter_tx_order::InterTxOrderChip;
use tabula_chips::poseidon::PoseidonChip;
use tabula_chips::range_check::RangeCheckChip;
use tabula_chips::smt_path::{SmtColPathChip, SmtTablePathChip};
use tabula_chips::state_column::StateColumnChip;
use tabula_chips::static_table::StaticTableChip;
use tabula_machine::AnyRap;
use tabula_stark::chips::core_chips;
use tabula_stark::trace::contributor::TraceContributor;

// ── Object-safety assertions (compile-time) ────────────────────────────────

/// Proves `AnyRap` is object-safe: if this compiles, `dyn AnyRap` works.
fn _assert_any_rap_object_safe(_: &dyn AnyRap) {}

/// Proves `TraceContributor` is object-safe after removing `Default` from `ChipSpec`.
fn _assert_trace_contributor_object_safe(_: &dyn TraceContributor) {}

// ── All 9 core chips implement AnyRap ──────────────────────────────────────

#[test]
fn all_core_chips_implement_any_rap() {
    let chips: Vec<Box<dyn AnyRap>> = vec![
        Box::new(ExecutionChip::<3>),
        Box::new(InterTxOrderChip::<3>),
        Box::new(StateColumnChip::<3>),
        Box::new(ColumnMetaChip),
        Box::new(PoseidonChip),
        Box::new(RangeCheckChip),
        Box::new(StaticTableChip::<3>),
        Box::new(SmtColPathChip),
        Box::new(SmtTablePathChip),
    ];

    let expected_ids = core_chips::ALL;
    assert_eq!(chips.len(), expected_ids.len());

    for (chip, expected_id) in chips.iter().zip(expected_ids.iter()) {
        assert_eq!(chip.chip_id(), *expected_id);
        // BaseAir<BabyBear>::width() callable through dyn AnyRap vtable
        assert!(<dyn AnyRap as BaseAir<BabyBear>>::width(chip.as_ref()) > 0);
    }
}

// ── Vtable dispatch: eval through &dyn AnyRap ─────────────────────────────

#[test]
fn eval_dispatch_via_dyn_any_rap() {
    // Use RangeCheckChip — simplest chip (2 columns, no interactions).
    let chip: &dyn AnyRap = &RangeCheckChip;

    // Verify metadata via vtable
    assert_eq!(chip.chip_id(), core_chips::RANGE_CHECK);
    assert!(!chip.has_interactions());

    let width = <dyn AnyRap as BaseAir<BabyBear>>::width(chip);
    assert_eq!(width, 2);

    // Eval with SymbolicAirBuilder via vtable dispatch.
    let mut builder = SymbolicAirBuilder::<BabyBear>::new(
        chip.preprocessed_width(),
        width,
        chip.num_public_values(),
        0, // permutation_width (unused for symbolic constraint extraction)
        0, // num_permutation_challenges
    );
    <dyn AnyRap as Air<SymbolicAirBuilder<BabyBear>>>::eval(chip, &mut builder);

    // If we got here without panic, vtable dispatch works.
}
