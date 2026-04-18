//! Auto-trait verification tests for the stable public machine surface.

fn assert_send<T: Send>() {}
fn assert_sync<T: Sync>() {}
fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn proof_types_are_send_sync() {
    use tabula_machine::{
        ChipOpening, ColumnProofEntry, ColumnSlotKey, PreparedColumnInput, PreparedMachineInput,
        PreparedTierInput, ProofTier, SubProofEnvelope, TabulaProof,
    };

    assert_send_sync::<TabulaProof>();
    assert_send_sync::<SubProofEnvelope>();
    assert_send_sync::<ColumnProofEntry>();
    assert_send_sync::<ChipOpening>();
    assert_send_sync::<ColumnSlotKey>();
    assert_send_sync::<PreparedColumnInput>();
    assert_send_sync::<PreparedTierInput>();
    assert_send_sync::<PreparedMachineInput>();
    assert_send_sync::<ProofTier>();
}

#[test]
fn error_types_are_send_sync() {
    use tabula_machine::{ProveError, SetupError, VerificationError};

    assert_send_sync::<ProveError>();
    assert_send_sync::<VerificationError>();
    assert_send_sync::<SetupError>();
}

#[test]
fn config_types_are_send_sync() {
    use tabula_machine::TabulaStarkConfig;

    assert_send_sync::<TabulaStarkConfig>();
}

#[test]
fn composition_types_are_send_sync() {
    use std::sync::Arc;

    use tabula_machine::{RootProofBackend, SmtRootProofBackend};

    assert_send_sync::<SmtRootProofBackend>();
    assert_send::<Box<dyn RootProofBackend>>();
    assert_sync::<Box<dyn RootProofBackend>>();
    assert_send::<Arc<dyn RootProofBackend>>();
    assert_sync::<Arc<dyn RootProofBackend>>();
}

#[test]
fn backend_surface_types_are_send_sync() {
    use std::sync::Arc;

    use tabula_machine::backend::ProofColumn;

    assert_send_sync::<Arc<dyn ProofColumn>>();
}

#[test]
fn machine_is_send_sync() {
    use tabula_machine::TabulaMachine;

    assert_send_sync::<TabulaMachine>();
}

#[test]
fn chip_identification_types_are_send_sync() {
    use tabula_stark::air::interaction::BusId;
    use tabula_stark::chips::{ChipId, ChipIdAllocator};

    assert_send_sync::<ChipId>();
    assert_send_sync::<BusId>();
    assert_send_sync::<ChipIdAllocator>();
}
