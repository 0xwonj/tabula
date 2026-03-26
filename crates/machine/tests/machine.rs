//! Tests for the stable machine surface.

mod common;

use common::dummy_proof_column;
use p3_field::PrimeCharacteristicRing;
use p3_koala_bear::KoalaBear;
use tabula_commitment::NativeDigest;
use tabula_machine::{
    PreparedMachineInput, PreparedTierInput, ProveError, PublicStatement, RootProofBackend,
    SmtRootProofBackend, TabulaMachine, default_config,
};
use tabula_stark::chips::core_chips;
use tabula_stark::trace::WitnessStore;

#[derive(Clone, Copy, Debug)]
struct CustomRootProofBackend;

impl RootProofBackend for CustomRootProofBackend {
    fn name(&self) -> &str {
        "custom_root_proof"
    }

    fn supported_root_binding_families(&self) -> &'static [tabula_core::RootProfileId] {
        SmtRootProofBackend.supported_root_binding_families()
    }

    fn airs(&self) -> Vec<Box<dyn tabula_machine::backend::AnyRap>> {
        SmtRootProofBackend.airs()
    }

    fn dyn_chips(&self) -> Vec<Box<dyn tabula_stark::trace::DynChip>> {
        SmtRootProofBackend.dyn_chips()
    }
}

#[test]
fn machine_new_creates_valid_machine() {
    let columns = vec![dummy_proof_column(0, 0)];

    let machine = TabulaMachine::new(columns).expect("machine creation");
    let debug = format!("{machine:?}");

    assert!(debug.contains("TabulaMachine"));
    assert!(debug.contains("num_columns"));
}

#[test]
fn with_config_uses_custom_config() {
    let columns = vec![dummy_proof_column(0, 0)];
    let custom_config = default_config();

    TabulaMachine::with_config(columns, custom_config).expect("machine with config");
}

#[test]
fn smt_root_proof_provides_two_chips() {
    let airs = SmtRootProofBackend.airs();
    assert_eq!(airs.len(), 2);
    assert_eq!(airs[0].chip_id(), core_chips::SMT_COL_PATH);
    assert_eq!(airs[1].chip_id(), core_chips::SMT_TABLE_PATH);
}

#[test]
fn direct_machine_prove_is_not_gated_by_runtime_root_authority() {
    let machine = TabulaMachine::builder()
        .with_root_proof_backend(CustomRootProofBackend)
        .build()
        .expect("machine build should allow custom proof-side root backends");

    let input = PreparedMachineInput {
        execution: PreparedTierInput {
            store: WitnessStore::new(),
        },
        columns: vec![],
        root: PreparedTierInput {
            store: WitnessStore::new(),
        },
        air_statement: PublicStatement {
            old_root: NativeDigest([KoalaBear::ZERO; 8]),
            new_root: NativeDigest([KoalaBear::ZERO; 8]),
        },
        semantic_statement_digest: [0u8; 32],
    };

    match machine.prove(input) {
        Err(ProveError::InvalidProofInput { detail }) => {
            assert!(!detail.contains("root witness"));
            assert!(!detail.contains("RootWitnessContract"));
        }
        Ok(_) => panic!("empty prepared tier inputs should not produce a valid proof"),
        Err(other) => panic!("unexpected error: {other}"),
    }
}
