//! Tests for the AnyRap trait: object safety, blanket impl coverage, vtable dispatch.
//!
//! Verifies that all chips used in the sharded architecture implement AnyRap.

use p3_air::{Air, BaseAir};
use p3_baby_bear::BabyBear;
use p3_uni_stark::SymbolicAirBuilder;

use tabula_chips::execution::ExecutionChip;
use tabula_chips::poseidon::PoseidonChip;
use tabula_chips::range_check::RangeCheckChip;
use tabula_chips::shards::memory::MemoryShardChip;
use tabula_chips::shards::meta::MetaShardChip;
use tabula_chips::shards::state::StateShardChip;
use tabula_chips::smt_path::{SmtColPathChip, SmtTablePathChip};
use tabula_chips::static_table::StaticTableChip;
use tabula_machine::AnyRap;
use tabula_stark::chips::{ChipIdAllocator, core_chips};

// ── Object-safety assertions (compile-time) ────────────────────────────────

/// Proves `AnyRap` is object-safe: if this compiles, `dyn AnyRap` works.
fn _assert_any_rap_object_safe(_: &dyn AnyRap) {}

// ── Execution tier chips implement AnyRap ──────────────────────────────────

#[test]
fn execution_tier_chips_implement_any_rap() {
    let chips: Vec<Box<dyn AnyRap>> = vec![
        Box::new(ExecutionChip::<3>),
        Box::new(StaticTableChip::<3>),
        Box::new(PoseidonChip),
        Box::new(RangeCheckChip),
    ];

    let expected = [
        core_chips::EXECUTION,
        core_chips::STATIC_TABLE,
        core_chips::POSEIDON,
        core_chips::RANGE_CHECK,
    ];

    assert_eq!(chips.len(), expected.len());
    for (chip, expected_id) in chips.iter().zip(expected.iter()) {
        assert_eq!(chip.chip_id(), *expected_id);
        assert!(<dyn AnyRap as BaseAir<BabyBear>>::width(chip.as_ref()) > 0);
    }
}

// ── Column tier (shard) chips implement AnyRap ─────────────────────────────

#[test]
fn column_tier_chips_implement_any_rap() {
    let mut alloc = ChipIdAllocator::for_shards();
    let mem_id = alloc.next();
    let state_id = alloc.next();
    let meta_id = alloc.next();

    let chips: Vec<Box<dyn AnyRap>> = vec![
        Box::new(MemoryShardChip::<3>::new(mem_id, 0, 0)),
        Box::new(StateShardChip::<3>::new(state_id, 0, 0)),
        Box::new(MetaShardChip::new(meta_id, 0, 0, 0, false)),
    ];

    let expected = [mem_id, state_id, meta_id];
    assert_eq!(chips.len(), expected.len());
    for (chip, expected_id) in chips.iter().zip(expected.iter()) {
        assert_eq!(chip.chip_id(), *expected_id);
        assert!(<dyn AnyRap as BaseAir<BabyBear>>::width(chip.as_ref()) > 0);
    }
}

// ── Root tier chips implement AnyRap ───────────────────────────────────────

#[test]
fn root_tier_chips_implement_any_rap() {
    let chips: Vec<Box<dyn AnyRap>> = vec![Box::new(SmtColPathChip), Box::new(SmtTablePathChip)];

    let expected = [core_chips::SMT_COL_PATH, core_chips::SMT_TABLE_PATH];
    assert_eq!(chips.len(), expected.len());
    for (chip, expected_id) in chips.iter().zip(expected.iter()) {
        assert_eq!(chip.chip_id(), *expected_id);
        assert!(<dyn AnyRap as BaseAir<BabyBear>>::width(chip.as_ref()) > 0);
    }
}

// ── Vtable dispatch: eval through &dyn AnyRap ──────────────────────────────

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
