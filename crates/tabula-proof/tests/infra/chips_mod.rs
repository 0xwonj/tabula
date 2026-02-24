use tabula_proof::air::chips::column_meta::ColumnMetaChip;
use tabula_proof::air::chips::execution::ExecutionChip;
use tabula_proof::air::chips::poseidon::PoseidonChip;
use tabula_proof::air::chips::range_check::RangeCheckChip;
use tabula_proof::air::chips::state_column::StateColumnChip;
use tabula_proof::air::{ChipSet, ChipSpec, TabulaAir};

#[test]
fn chip_meta_name() {
    let chip = ColumnMetaChip;
    assert_eq!(chip.chip_name(), "ColumnMeta");
}

#[test]
fn tabula_air_delegates_chip_meta() {
    let air = TabulaAir::ColumnMeta(ColumnMetaChip);
    assert_eq!(air.chip_name(), "ColumnMeta");
}

#[test]
fn tabula_air_range_check() {
    let air = TabulaAir::RangeCheck(RangeCheckChip);
    assert_eq!(air.chip_name(), "RangeCheck");
}

#[test]
fn tabula_air_poseidon() {
    let air = TabulaAir::Poseidon(PoseidonChip);
    assert_eq!(air.chip_name(), "Poseidon");
}

#[test]
fn tabula_air_execution() {
    let air = TabulaAir::Execution(ExecutionChip::<3>);
    assert_eq!(air.chip_name(), "Execution");
}

#[test]
fn tabula_air_state_column() {
    let air = TabulaAir::StateColumn(StateColumnChip::<3>);
    assert_eq!(air.chip_name(), "StateColumn");
}

#[test]
fn chip_set_all_chips() {
    let chips = TabulaAir::all_chips();
    assert_eq!(chips.len(), 9);
}

#[test]
fn chip_set_from_name() {
    assert!(TabulaAir::from_name("Execution").is_some());
    assert!(TabulaAir::from_name("ColumnMeta").is_some());
    assert!(TabulaAir::from_name("Poseidon").is_some());
    assert!(TabulaAir::from_name("NonExistent").is_none());
}

#[test]
fn chip_set_chip_names() {
    let names = TabulaAir::chip_names();
    assert_eq!(names.len(), 9);
    assert!(names.contains(&"Execution"));
    assert!(names.contains(&"SmtTablePath"));
}

#[test]
fn chip_meta_public_values() {
    use tabula_proof::air::chips::smt_path::SmtTablePathChip;
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
