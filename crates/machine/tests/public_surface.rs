//! Guardrails for the stable public machine surface.

use std::fs;
use std::path::Path;

fn crate_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
}

fn read_crate_file(rel: &str) -> String {
    fs::read_to_string(crate_root().join(rel)).expect("read crate file")
}

#[test]
fn root_surface_uses_prepared_input_names_only() {
    let lib_rs = read_crate_file("src/lib.rs");

    for forbidden in [
        "MachineProofInput",
        "PreparedColumnStore",
        "MachineTopology",
        "ProofTopology",
        "TierTopology",
        "TierProvingMetadata",
        "TierVerificationMetadata",
        "ChipRegistry",
        "RegisteredChip",
        "compute_external_buses",
    ] {
        assert!(
            !lib_rs.contains(forbidden),
            "machine root must not expose removed internal surface '{forbidden}'"
        );
    }

    assert!(lib_rs.contains("PreparedMachineInput"));
    assert!(lib_rs.contains("PreparedColumnInput"));
    assert!(lib_rs.contains("PreparedTierInput"));
    assert!(lib_rs.contains("ColumnSlotKey"));
    assert!(lib_rs.contains("RootProofBackend"));
    assert!(lib_rs.contains("SmtRootProofBackend"));
    assert!(!lib_rs.contains("RootWitnessContract"));
}

#[test]
fn backend_prelude_stays_authoring_only() {
    let prelude_rs = read_crate_file("src/backend/prelude.rs");

    for forbidden in [
        "ChipRegistry",
        "RegisteredChip",
        "TierProvingMetadata",
        "TierVerificationMetadata",
        "MachineTopology",
        "ProofTopology",
        "TierTopology",
    ] {
        assert!(
            !prelude_rs.contains(forbidden),
            "backend prelude must not expose internal topology or metadata '{forbidden}'"
        );
    }
}

#[test]
fn input_assembly_does_not_partition_shared_store_by_labels() {
    let assembly_rs = read_crate_file("src/input/assembly.rs");

    for forbidden in [
        "ROOT_LABELS",
        "partition_by_tier",
        "labels_for_root_witness_contract",
        "drain_labels(",
        "RootWitnessContract",
    ] {
        assert!(
            !assembly_rs.contains(forbidden),
            "machine input assembly must not retain legacy shared-store partition helper '{forbidden}'"
        );
    }
}
