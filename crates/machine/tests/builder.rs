//! Tests for the machine builder.

mod common;

use common::dummy_proof_column;
use tabula_machine::{SetupError, SmtRootProof, TabulaMachine};
use tabula_stark::chips::core_chips;

#[test]
fn builder_creates_valid_machine() {
    let machine = TabulaMachine::builder()
        .with_columns(vec![dummy_proof_column(0, 0)])
        .build()
        .expect("builder should create a valid machine");

    let setups = machine.setup().proof_setups();
    assert_eq!(setups.execution.registry.chip_ids().len(), 3);
    assert_eq!(setups.columns.len(), 1);
    assert_eq!(setups.root.registry.chip_ids().len(), 4);
}

#[test]
fn builder_with_config() {
    let config = tabula_machine::default_config();
    let machine = TabulaMachine::builder()
        .with_columns(vec![dummy_proof_column(0, 0)])
        .with_config(config)
        .build()
        .expect("builder with config");

    assert_eq!(
        machine
            .setup()
            .proof_setups()
            .execution
            .registry
            .chip_ids()
            .len(),
        3
    );
}

#[test]
fn builder_with_custom_root_proof() {
    let machine = TabulaMachine::builder()
        .with_columns(vec![dummy_proof_column(0, 0)])
        .with_root_proof(SmtRootProof)
        .build()
        .expect("builder with custom root proof");

    let root_ids = machine.setup().proof_setups().root.registry.chip_ids();
    assert!(root_ids.contains(&core_chips::SMT_COL_PATH));
    assert!(root_ids.contains(&core_chips::SMT_TABLE_PATH));
}

#[test]
fn builder_no_columns() {
    let machine = TabulaMachine::builder()
        .build()
        .expect("builder with no columns");

    assert_eq!(machine.setup().proof_setups().columns.len(), 0);
}

#[test]
fn direct_constructor_matches_builder() {
    let columns = vec![dummy_proof_column(0, 0), dummy_proof_column(0, 1)];

    let direct = TabulaMachine::new(columns.clone()).expect("direct machine");
    let built = TabulaMachine::builder()
        .with_columns(columns)
        .build()
        .expect("builder");

    assert_eq!(
        direct.setup().proof_setups().execution.registry.chip_ids(),
        built.setup().proof_setups().execution.registry.chip_ids()
    );
    assert_eq!(
        direct.setup().proof_setups().columns.len(),
        built.setup().proof_setups().columns.len()
    );
    assert_eq!(
        direct.setup().proof_setups().root.registry.chip_ids(),
        built.setup().proof_setups().root.registry.chip_ids()
    );
}

#[test]
fn build_setup_round_trips_through_machine() {
    let setup = TabulaMachine::builder()
        .with_columns(vec![dummy_proof_column(0, 0)])
        .build_setup()
        .expect("machine setup");

    let machine = TabulaMachine::from_setup(setup);
    let setups = machine.setup().proof_setups();

    assert_eq!(setups.execution.registry.chip_ids().len(), 3);
    assert_eq!(setups.columns.len(), 1);
    assert_eq!(setups.root.registry.chip_ids().len(), 4);
}

mod test_extension {
    use tabula_core::error::TabulaError;
    use tabula_machine::backend::extension::ExecutionTierExtension;
    use tabula_machine::backend::prelude::*;
    use tabula_stark::trace::trace_map::TraceMap;

    #[derive(Clone, Debug)]
    pub struct DummyChip;

    pub const DUMMY_CHIP_ID: ChipId = ChipId(200);

    impl ChipSpec for DummyChip {
        fn chip_id(&self) -> ChipId {
            DUMMY_CHIP_ID
        }

        fn chip_name(&self) -> &'static str {
            "DummyChip"
        }

        fn has_interactions(&self) -> bool {
            false
        }
    }

    impl<F> BaseAir<F> for DummyChip {
        fn width(&self) -> usize {
            2
        }
    }

    impl<AB: AirBuilder> Air<AB> for DummyChip {
        fn eval(&self, _builder: &mut AB) {}
    }

    impl TraceContributor for DummyChip {
        fn phase(&self) -> TracePhase {
            TracePhase::INDEPENDENT
        }

        fn contribute(&self, _store: &WitnessStore, map: &mut TraceMap) -> Result<(), TabulaError> {
            let trace = RowMajorMatrix::new(vec![KoalaBear::ZERO; 2], 2);
            map.insert(self.chip_id(), trace);
            Ok(())
        }
    }

    pub struct DummyExtension;

    impl ExecutionTierExtension for DummyExtension {
        fn name(&self) -> &str {
            "dummy-extension"
        }

        fn airs(&self) -> Vec<Box<dyn AnyRap>> {
            vec![Box::new(DummyChip)]
        }

        fn dyn_chips(&self) -> Vec<Box<dyn DynChip>> {
            vec![Box::new(DummyChip)]
        }
    }
}

#[test]
fn builder_with_extension_registers_chip() {
    use test_extension::{DUMMY_CHIP_ID, DummyExtension};

    let machine = TabulaMachine::builder()
        .with_backend_execution_extension(DummyExtension)
        .build()
        .expect("builder with extension");

    let exec_ids = machine.setup().proof_setups().execution.registry.chip_ids();
    assert!(exec_ids.contains(&core_chips::EXECUTION));
    assert!(exec_ids.contains(&core_chips::STATIC_TABLE));
    assert!(exec_ids.contains(&core_chips::RANGE_CHECK));
    assert!(exec_ids.contains(&DUMMY_CHIP_ID));
    assert_eq!(exec_ids.len(), 4);
}

#[test]
fn builder_rejects_duplicate_chip_id() {
    use test_extension::DummyExtension;

    let result = TabulaMachine::builder()
        .with_backend_execution_extension(DummyExtension)
        .with_backend_execution_extension(DummyExtension)
        .build();

    assert!(matches!(result, Err(SetupError::DuplicateChipId(_))));
}
