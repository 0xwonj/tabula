use tabula_chips::column_meta::ColumnMetaChip;
use tabula_chips::execution::ExecutionChip;
use tabula_chips::poseidon::PoseidonChip;
use tabula_chips::range_check::RangeCheckChip;
use tabula_chips::state_column::StateColumnChip;
use tabula_chips::{ChipSpec, TabulaAir, core_chips};
use tabula_stark::air::ChipSet;

#[test]
fn chip_meta_id() {
    let chip = ColumnMetaChip;
    assert_eq!(chip.chip_id(), core_chips::COLUMN_META);
    assert_eq!(chip.chip_name(), "ColumnMeta");
}

#[test]
fn tabula_air_delegates_chip_meta() {
    let air = TabulaAir::ColumnMeta(ColumnMetaChip);
    assert_eq!(air.chip_id(), core_chips::COLUMN_META);
}

#[test]
fn tabula_air_range_check() {
    let air = TabulaAir::RangeCheck(RangeCheckChip);
    assert_eq!(air.chip_id(), core_chips::RANGE_CHECK);
}

#[test]
fn tabula_air_poseidon() {
    let air = TabulaAir::Poseidon(PoseidonChip);
    assert_eq!(air.chip_id(), core_chips::POSEIDON);
}

#[test]
fn tabula_air_execution() {
    let air = TabulaAir::Execution(ExecutionChip::<3>);
    assert_eq!(air.chip_id(), core_chips::EXECUTION);
}

#[test]
fn tabula_air_state_column() {
    let air = TabulaAir::StateColumn(StateColumnChip::<3>);
    assert_eq!(air.chip_id(), core_chips::STATE_COLUMN);
}

#[test]
fn chip_set_all_chips() {
    let chips = TabulaAir::all_chips();
    assert_eq!(chips.len(), 9);
}

#[test]
fn chip_set_from_id() {
    assert!(TabulaAir::from_id(core_chips::EXECUTION).is_some());
    assert!(TabulaAir::from_id(core_chips::COLUMN_META).is_some());
    assert!(TabulaAir::from_id(core_chips::POSEIDON).is_some());
}

#[test]
fn chip_set_chip_ids() {
    let ids = TabulaAir::chip_ids();
    assert_eq!(ids.len(), 9);
    assert!(ids.contains(&core_chips::EXECUTION));
    assert!(ids.contains(&core_chips::SMT_TABLE_PATH));
}

#[test]
fn chip_meta_public_values() {
    use tabula_chips::smt_path::SmtTablePathChip;
    let smt = SmtTablePathChip;
    assert_eq!(smt.num_public_values(), 16); // old_root[8] + new_root[8]

    let exec = ExecutionChip::<3>;
    assert_eq!(exec.num_public_values(), 0);
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
