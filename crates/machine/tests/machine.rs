//! Tests for ChipRegistry, TabulaMachine, and proof setup.

mod common;

use tabula_chips::range_check::RangeCheckChip;
use tabula_core::{ColId, TableId};
use tabula_machine::{ChipRegistry, SetupError, TabulaMachine, default_config};

// ── ChipRegistry standalone ────────────────────────────────────────────────

#[test]
fn registry_validate_empty() {
    let reg = ChipRegistry::new();
    assert!(matches!(reg.validate(), Err(SetupError::EmptyRegistry)));
}

#[test]
fn registry_validate_duplicate() {
    let mut reg = ChipRegistry::new();
    reg.register(RangeCheckChip);
    reg.register(RangeCheckChip);
    assert!(matches!(
        reg.validate(),
        Err(SetupError::DuplicateChipId(_))
    ));
}

#[test]
fn registry_validate_ok() {
    let mut reg = ChipRegistry::new();
    reg.register(RangeCheckChip);
    assert!(reg.validate().is_ok());
    assert_eq!(reg.chip_ids().len(), 1);
}

// ── TabulaMachine creation ──────────────────────────────────────────────────

#[test]
fn machine_new_creates_valid_machine() {
    use tabula_commitment::scheme_tags;
    use tabula_machine::ColumnSetupConfig;

    let col_configs = vec![ColumnSetupConfig {
        table_id: TableId(0),
        col_id: ColId(0),
        scheme_tag: scheme_tags::SSMC,
        receives_commitment: true,
    }];

    let machine = TabulaMachine::new(&col_configs).expect("machine creation");
    let setups = machine.setups();

    // Execution tier: 4 chips
    assert_eq!(setups.execution.registry.chip_ids().len(), 4);
    // One column tier: 6 chips
    assert_eq!(setups.columns.len(), 1);
    assert_eq!(setups.columns[0].1.registry.chip_ids().len(), 6);
    // Root tier: 4 chips
    assert_eq!(setups.root.registry.chip_ids().len(), 4);
}

#[test]
fn with_config_uses_custom_config() {
    use tabula_commitment::scheme_tags;
    use tabula_machine::ColumnSetupConfig;

    let col_configs = vec![ColumnSetupConfig {
        table_id: TableId(0),
        col_id: ColId(0),
        scheme_tag: scheme_tags::SSMC,
        receives_commitment: true,
    }];

    let custom_config = default_config();
    let machine =
        TabulaMachine::with_config(&col_configs, custom_config).expect("machine with config");

    // Machine should be functional with custom config.
    assert_eq!(machine.setups().execution.registry.chip_ids().len(), 4);
}

// ── RootProof trait ─────────────────────────────────────────────────────────

#[test]
fn smt_root_proof_provides_two_chips() {
    use tabula_machine::{RootProof, SmtRootProof};
    use tabula_stark::chips::core_chips;

    let airs = SmtRootProof.airs();
    assert_eq!(airs.len(), 2);
    assert_eq!(airs[0].chip_id(), core_chips::SMT_COL_PATH);
    assert_eq!(airs[1].chip_id(), core_chips::SMT_TABLE_PATH);
}
