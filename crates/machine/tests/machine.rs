//! Tests for ChipRegistry, TabulaMachine, and proof setup.

mod common;

use common::dummy_proof_column;
use tabula_chips::range_check::RangeCheckChip;
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
    let columns = vec![dummy_proof_column(0, 0)];

    let machine = TabulaMachine::new(columns.clone()).expect("machine creation");
    let setups = machine.setup().proof_setups();

    // Execution tier: 4 chips
    assert_eq!(setups.execution.registry.chip_ids().len(), 4);
    // One column tier: backend-added Poseidon + RangeCheck only.
    assert_eq!(setups.columns.len(), 1);
    assert_eq!(setups.columns[0].1.registry.chip_ids().len(), 2);
    // Root tier: 4 chips
    assert_eq!(setups.root.registry.chip_ids().len(), 4);
}

#[test]
fn with_config_uses_custom_config() {
    let columns = vec![dummy_proof_column(0, 0)];

    let custom_config = default_config();
    let machine =
        TabulaMachine::with_config(columns.clone(), custom_config).expect("machine with config");

    // Machine should be functional with custom config.
    assert_eq!(
        machine
            .setup()
            .proof_setups()
            .execution
            .registry
            .chip_ids()
            .len(),
        4
    );
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
