//! Tests for the stable machine builder surface.

mod common;

use common::dummy_proof_column;
use std::sync::Arc;
use tabula_machine::{SetupError, SmtRootProofBackend, TabulaMachine};

#[test]
fn builder_creates_valid_machine() {
    let machine = TabulaMachine::builder()
        .with_columns(vec![dummy_proof_column(0, 0)])
        .build()
        .expect("builder should create a valid machine");

    let debug = format!("{machine:?}");
    assert!(debug.contains("TabulaMachine"));
    assert!(debug.contains("num_columns"));
}

#[test]
fn builder_with_config() {
    let config = tabula_machine::default_config();
    TabulaMachine::builder()
        .with_columns(vec![dummy_proof_column(0, 0)])
        .with_config(config)
        .build()
        .expect("builder with config");
}

#[test]
fn builder_with_custom_root_proof() {
    TabulaMachine::builder()
        .with_columns(vec![dummy_proof_column(0, 0)])
        .with_root_proof_backend(SmtRootProofBackend)
        .build()
        .expect("builder with custom root proof");
}

#[test]
fn builder_with_shared_root_proof_backend_arc() {
    TabulaMachine::builder()
        .with_columns(vec![dummy_proof_column(0, 0)])
        .with_root_proof_backend_arc(Arc::new(SmtRootProofBackend))
        .build()
        .expect("builder with shared root proof backend");
}

#[test]
fn builder_no_columns() {
    TabulaMachine::builder()
        .build()
        .expect("builder with no columns");
}

#[test]
fn direct_constructor_matches_builder() {
    let columns = vec![dummy_proof_column(0, 0), dummy_proof_column(0, 1)];

    let direct = TabulaMachine::new(columns.clone()).expect("direct machine");
    let built = TabulaMachine::builder()
        .with_columns(columns)
        .build()
        .expect("builder");

    assert_eq!(format!("{direct:?}"), format!("{built:?}"));
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
fn builder_with_extension_builds_machine() {
    use test_extension::DummyExtension;

    TabulaMachine::builder()
        .with_backend_execution_extension(DummyExtension)
        .build()
        .expect("builder with extension");
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
