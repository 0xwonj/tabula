use tabula_proof::air::chips::column_meta::ColumnMetaChip;
use tabula_proof::air::chips::execution::ExecutionChip;
use tabula_proof::air::chips::merge::GlobalMergeChip;
use tabula_proof::air::chips::poseidon::PoseidonChip;
use tabula_proof::air::chips::range_check::RangeCheckChip;
use tabula_proof::air::chips::ssmc::GlobalSsmcChip;
use tabula_proof::air::{ChipMeta, TabulaAir};

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
fn tabula_air_ssmc() {
    let air = TabulaAir::SsmcStandard(GlobalSsmcChip::<3>);
    assert_eq!(air.chip_name(), "GlobalSSMC");
}

#[test]
fn tabula_air_merge() {
    let air = TabulaAir::MergeStandard(GlobalMergeChip::<3>);
    assert_eq!(air.chip_name(), "GlobalMerge");
}

#[test]
fn tabula_air_poseidon() {
    let air = TabulaAir::Poseidon(PoseidonChip);
    assert_eq!(air.chip_name(), "Poseidon");
}

#[test]
fn tabula_air_execution() {
    let air = TabulaAir::ExecutionStandard(ExecutionChip::<3>);
    assert_eq!(air.chip_name(), "Execution");
}
