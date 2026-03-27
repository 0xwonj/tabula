use p3_air::BaseAir;
use tabula_chips::execution::ExecutionChip;
use tabula_chips::poseidon::PoseidonChip;
use tabula_chips::range_check::RangeCheckChip;
use tabula_chips::{ChipSpec, core_chips, core_dyn_chips};

#[test]
fn chip_spec_delegates() {
    let exec = ExecutionChip::<3>;
    assert_eq!(exec.chip_id(), core_chips::EXECUTION);

    let rc = RangeCheckChip;
    assert_eq!(rc.chip_id(), core_chips::RANGE_CHECK);

    let pos = PoseidonChip;
    assert_eq!(pos.chip_id(), core_chips::POSEIDON);
}

#[test]
fn core_dyn_chips_returns_six() {
    let chips = core_dyn_chips();
    assert_eq!(chips.len(), 6);
}

#[test]
fn core_dyn_chips_ids_match() {
    let chips = core_dyn_chips();
    let ids: Vec<_> = chips.iter().map(|c| c.chip_id()).collect();
    assert_eq!(ids, core_chips::ALL.to_vec());
}

#[test]
fn chip_meta_public_values() {
    use tabula_chips::smt_path::SmtTablePathChip;
    let smt = SmtTablePathChip;
    assert_eq!(
        BaseAir::<p3_koala_bear::KoalaBear>::num_public_values(&smt),
        16
    ); // old_root[8] + new_root[8]

    let exec = ExecutionChip::<3>;
    assert_eq!(
        BaseAir::<p3_koala_bear::KoalaBear>::num_public_values(&exec),
        0
    );
}

#[test]
fn chip_spec_preprocessed_width() {
    let poseidon = PoseidonChip;
    assert!(poseidon.preprocessed_width() > 0);

    let exec = ExecutionChip::<3>;
    assert_eq!(exec.preprocessed_width(), 0);
}

#[test]
fn chip_spec_has_interactions() {
    let poseidon = PoseidonChip;
    assert!(poseidon.has_interactions());

    let range_check = RangeCheckChip;
    assert!(!range_check.has_interactions());

    let exec = ExecutionChip::<3>;
    assert!(exec.has_interactions());
}
