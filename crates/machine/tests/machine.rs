//! Tests for ChipRegistry, TabulaMachine, and MachineBuilder.

mod common;

use tabula_chips::range_check::RangeCheckChip;
use tabula_core::{ColId, RowKey, TableId, Value};
use tabula_machine::{
    ChipRegistry, CommitmentScheme, SetupError, SsmcScheme, TabulaMachine, core_chips,
    default_commitment_chips, default_config,
};
use tabula_stark::air::interaction::BusId;
use tabula_stark::chips::core_chips as core_chip_ids;

use common::{build_traces_from_source, default_machine, make_tx, prove_and_verify};

// ── Builder basics ───────────────────────────────────────────────────────────

#[test]
fn builder_with_core_chips_registers_layer0() {
    let machine = TabulaMachine::builder()
        .with_core_chips()
        .build()
        .expect("core chip registration should succeed");

    let ids = machine.registry().chip_ids();
    assert_eq!(ids.len(), 8, "Layer 0 has 8 chips");
    // Compare as sets — registration order may differ from canonical order.
    let mut sorted = ids.clone();
    sorted.sort();
    let mut expected = core_chip_ids::LAYER0.to_vec();
    expected.sort();
    assert_eq!(sorted, expected);
}

#[test]
fn builder_with_default_commitments_registers_all_nine() {
    let machine = TabulaMachine::builder()
        .with_core_chips()
        .with_default_commitments()
        .build()
        .expect("full registration should succeed");

    let ids = machine.registry().chip_ids();
    assert_eq!(ids.len(), 9);
    assert!(
        ids.contains(&core_chip_ids::STATE_COLUMN),
        "default commitments must include StateColumn"
    );
}

#[test]
fn empty_registry_fails() {
    let result = TabulaMachine::builder().build();
    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, SetupError::EmptyRegistry),
        "expected EmptyRegistry, got: {err}"
    );
}

#[test]
fn duplicate_chip_fails() {
    let result = TabulaMachine::builder()
        .with_chip(RangeCheckChip)
        .with_chip(RangeCheckChip)
        .build();

    assert!(result.is_err());
    let err = result.unwrap_err();
    assert!(
        matches!(err, SetupError::DuplicateChipId(_)),
        "expected DuplicateChipId, got: {err}"
    );
}

// ── Registry contents ────────────────────────────────────────────────────────

#[test]
fn registry_contains_all_core_ids() {
    let machine = TabulaMachine::builder()
        .with_core_chips()
        .with_default_commitments()
        .build()
        .expect("build should succeed");

    let mut ids = machine.registry().chip_ids();
    ids.sort();
    let mut expected = core_chip_ids::ALL.to_vec();
    expected.sort();
    assert_eq!(ids, expected);
}

#[test]
fn registry_get_returns_correct_chip() {
    let machine = TabulaMachine::builder()
        .with_core_chips()
        .with_default_commitments()
        .build()
        .expect("build should succeed");

    let chip = machine
        .registry()
        .get(core_chip_ids::RANGE_CHECK)
        .expect("RangeCheck should be registered");
    assert_eq!(chip.chip_id(), core_chip_ids::RANGE_CHECK);
    assert!(!chip.has_interactions());

    assert!(
        machine
            .registry()
            .get(tabula_stark::chips::ChipId(999))
            .is_none()
    );
}

// ── Custom chip registration ────────────────────────────────────────────────

#[test]
fn with_chip_registers_single_chip() {
    let machine = TabulaMachine::builder()
        .with_chip(RangeCheckChip)
        .build()
        .expect("single chip should succeed");

    assert_eq!(
        machine.registry().chip_ids(),
        vec![core_chip_ids::RANGE_CHECK]
    );
}

// ── Bus consumer registration ────────────────────────────────────────────────

#[test]
fn with_bus_consumer_registers_in_all_layers() {
    let machine = TabulaMachine::builder()
        .with_core_chips()
        .with_default_commitments()
        .build()
        .expect("full registration should succeed");

    // RangeCheckChip is registered via with_bus_consumers() inside with_core_chips().
    // Verify it appears in the registry (AIR layer).
    let ids = machine.registry().chip_ids();
    assert!(
        ids.contains(&core_chip_ids::RANGE_CHECK),
        "RangeCheck should be in registry"
    );
}

#[test]
fn with_bus_consumer_standalone() {
    // Register a single bus consumer without core chips.
    let machine = TabulaMachine::builder()
        .with_bus_consumer(RangeCheckChip)
        .build()
        .expect("single bus consumer should succeed");

    assert_eq!(
        machine.registry().chip_ids(),
        vec![core_chip_ids::RANGE_CHECK]
    );
}

#[test]
fn with_bus_consumer_does_not_duplicate_with_core() {
    // with_core_chips() already registers RangeCheckChip.
    // Adding it again via with_bus_consumer should fail on duplicate.
    let result = TabulaMachine::builder()
        .with_core_chips()
        .with_bus_consumer(RangeCheckChip)
        .build();
    assert!(
        matches!(result, Err(SetupError::DuplicateChipId(_))),
        "expected DuplicateChipId, got: {:?}",
        result
    );
}

// ── core_chips() function ────────────────────────────────────────────────────

#[test]
fn core_chips_returns_layer0() {
    let chips = core_chips();
    assert_eq!(chips.len(), 8, "core_chips() returns Layer 0 only");
    let mut ids: Vec<_> = chips.iter().map(|c| c.chip_id()).collect();
    ids.sort();
    let mut expected = core_chip_ids::LAYER0.to_vec();
    expected.sort();
    assert_eq!(ids, expected);
}

#[test]
fn default_commitment_chips_returns_ssmc() {
    let chips = default_commitment_chips();
    assert_eq!(chips.len(), 1);
    assert_eq!(chips[0].chip_id(), core_chip_ids::STATE_COLUMN);
}

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
    reg.register_all_defaults();
    assert!(reg.validate().is_ok());
}

// ── DynChipWithoutAir ─────────────────────────────────────────────────────────

#[test]
fn dyn_chip_without_air_fails() {
    use tabula_chips::state_column::StateColumnChip;
    use tabula_machine::AnyRap;
    use tabula_stark::trace::DynChip;

    // A broken commitment scheme that returns a DynChip but no AIR.
    // This simulates a buggy CommitmentScheme implementation.
    struct BrokenScheme;
    impl CommitmentScheme for BrokenScheme {
        fn airs(&self) -> Vec<Box<dyn AnyRap>> {
            vec![] // no AIR registered
        }
        fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> {
            vec![Box::new(StateColumnChip::<3>)] // DynChip without matching AIR
        }
    }

    let result = TabulaMachine::builder()
        .with_chip(RangeCheckChip) // need at least one AIR to pass EmptyRegistry
        .with_commitment(&BrokenScheme)
        .build();

    assert!(
        matches!(result, Err(SetupError::DynChipWithoutAir(_))),
        "expected DynChipWithoutAir, got: {:?}",
        result
    );
}

// ── Prove/verify via TabulaMachine ───────────────────────────────────────────

#[test]
fn machine_prove_verify_read_write() {
    let source = "\
table accounts { balance: u64 }
tx touch(id: u64) {
    let bal = accounts[id].balance
    accounts[id].balance = bal
}";
    prove_and_verify(
        source,
        &[(TableId(0), ColId(0), RowKey(10), Value::U64(50))],
        vec![make_tx(vec![Value::U64(10)])],
    );
}

// ── debug_validate ───────────────────────────────────────────────────────────

#[test]
fn debug_validate_passes_on_valid_traces() {
    let source = "\
table t { val: u64 }
tx op(id: u64) {
    let x = t[id].val
    t[id].val = x
}";
    let machine = default_machine();
    let (traces, _statement) = build_traces_from_source(
        &machine,
        source,
        &[(TableId(0), ColId(0), RowKey(1), Value::U64(42))],
        vec![make_tx(vec![Value::U64(1)])],
    );

    machine
        .debug_validate(&traces)
        .expect("debug_validate should pass on valid traces");
}

// ── with_config ──────────────────────────────────────────────────────────────

#[test]
fn with_config_uses_custom_config() {
    // Build a machine with an explicitly provided config (same as default).
    // Verify it produces the same results as the default.
    let custom_config = default_config();
    let machine = TabulaMachine::builder()
        .with_core_chips()
        .with_default_commitments()
        .with_config(custom_config)
        .build()
        .expect("build with custom config should succeed");

    // The machine should be functional — prove/verify a simple pipeline.
    let source = "\
table t { val: u64 }
tx op(id: u64) {
    let x = t[id].val
    t[id].val = x
}";
    let (traces, statement) = build_traces_from_source(
        &machine,
        source,
        &[(TableId(0), ColId(0), RowKey(1), Value::U64(7))],
        vec![make_tx(vec![Value::U64(1)])],
    );
    let proof = machine.prove(&traces, statement).expect("proving");
    machine.verify(&proof).expect("verification");
}

// ── with_buses ───────────────────────────────────────────────────────────────

#[test]
fn with_buses_adds_custom_bus_ids() {
    // Custom bus IDs are additive — core buses are always present.
    let custom_bus = BusId(500);
    let machine = TabulaMachine::builder()
        .with_core_chips()
        .with_default_commitments()
        .with_buses([custom_bus])
        .build()
        .expect("build with custom buses should succeed");

    // Machine builds successfully with extra buses.
    assert_eq!(machine.registry().chip_ids().len(), 9);
}

// ── CommitmentScheme ─────────────────────────────────────────────────────────

#[test]
fn with_commitment_registers_scheme_chips() {
    // with_commitment(&SsmcScheme) should register StateColumnChip (1 chip).
    let machine = TabulaMachine::builder()
        .with_core_chips()
        .with_commitment(&SsmcScheme)
        .build()
        .expect("build with explicit commitment should succeed");

    let ids = machine.registry().chip_ids();
    assert_eq!(ids.len(), 9, "8 core + 1 SSMC = 9 chips");
    assert!(
        ids.contains(&core_chip_ids::STATE_COLUMN),
        "SsmcScheme should register StateColumn chip"
    );
}

#[test]
fn with_default_commitments_equivalent_to_with_commitment() {
    // with_default_commitments() should produce the same chip set as
    // with_commitment(&SsmcScheme).
    let machine_default = TabulaMachine::builder()
        .with_core_chips()
        .with_default_commitments()
        .build()
        .expect("default build");

    let machine_explicit = TabulaMachine::builder()
        .with_core_chips()
        .with_commitment(&SsmcScheme)
        .build()
        .expect("explicit build");

    let mut ids_default = machine_default.registry().chip_ids();
    let mut ids_explicit = machine_explicit.registry().chip_ids();
    ids_default.sort();
    ids_explicit.sort();
    assert_eq!(ids_default, ids_explicit);
}

#[test]
fn ssmc_scheme_provides_state_column_chip() {
    let airs = SsmcScheme.airs();
    assert_eq!(airs.len(), 1);
    assert_eq!(
        airs[0].chip_id(),
        core_chip_ids::STATE_COLUMN,
        "SsmcScheme should provide StateColumnChip"
    );
}

#[test]
fn smt_scheme_provides_no_chips() {
    use tabula_machine::SmtScheme;
    let airs = SmtScheme.airs();
    assert!(airs.is_empty(), "SmtScheme should have no chips");
}

#[test]
fn with_commitment_prove_verify() {
    // Full prove/verify pipeline using with_commitment() instead of
    // with_default_commitments().
    let machine = TabulaMachine::builder()
        .with_core_chips()
        .with_commitment(&SsmcScheme)
        .build()
        .expect("build");

    let source = "\
table t { val: u64 }
tx op(id: u64) {
    let x = t[id].val
    t[id].val = x
}";
    let (traces, statement) = build_traces_from_source(
        &machine,
        source,
        &[(TableId(0), ColId(0), RowKey(1), Value::U64(42))],
        vec![make_tx(vec![Value::U64(1)])],
    );
    let proof = machine.prove(&traces, statement).expect("proving");
    machine
        .verify(&proof)
        .expect("verification with explicit commitment should succeed");
}
