//! Auto-trait verification tests.
//!
//! Ensures key public types remain Send + Sync, preventing accidental
//! introduction of non-thread-safe internals (e.g., `Rc`, `Cell`).
//!
//! Pattern borrowed from Triton VM: a compile-time assertion via
//! `assert_send_sync::<T>()` catches regressions at build time, not
//! at runtime, making it free to maintain.

fn assert_send<T: Send>() {}
fn assert_sync<T: Sync>() {}
fn assert_send_sync<T: Send + Sync>() {}

// ── Machine-layer proof types ───────────────────────────────────────────────

#[test]
fn proof_types_are_send_sync() {
    use tabula_machine::{
        ChipOpening, ColumnIdentity, ColumnProofEntry, ProofTier, SubProofEnvelope, TabulaProof,
    };

    assert_send_sync::<TabulaProof>();
    assert_send_sync::<SubProofEnvelope>();
    assert_send_sync::<ColumnProofEntry>();
    assert_send_sync::<ChipOpening>();
    assert_send_sync::<ColumnIdentity>();
    assert_send_sync::<ProofTier>();
}

// ── Machine-layer error types ───────────────────────────────────────────────

#[test]
fn error_types_are_send_sync() {
    use tabula_machine::{ProveError, SetupError, VerificationError};

    assert_send_sync::<ProveError>();
    assert_send_sync::<VerificationError>();
    assert_send_sync::<SetupError>();
}

// ── Machine-layer key types ─────────────────────────────────────────────────

#[test]
fn key_types_are_send_sync() {
    use tabula_machine::{TabulaProvingKey, TabulaVerifyingKey};

    assert_send_sync::<TabulaProvingKey>();
    assert_send_sync::<TabulaVerifyingKey>();
}

// ── Machine-layer setup types ───────────────────────────────────────────────

#[test]
fn setup_types_are_send_sync() {
    use tabula_machine::{ColumnSetupConfig, ProofSetups, ProofTraces, TierSetup};

    assert_send_sync::<ColumnSetupConfig>();
    assert_send_sync::<ProofTraces>();
    // TierSetup and ProofSetups contain `Box<dyn DynChip>` / `Box<dyn BusConsumer>`;
    // they must be Send + Sync for parallel proof construction.
    assert_send_sync::<TierSetup>();
    assert_send_sync::<ProofSetups>();
}

// ── Machine-layer config types ──────────────────────────────────────────────

#[test]
fn config_types_are_send_sync() {
    use tabula_machine::TabulaStarkConfig;

    assert_send_sync::<TabulaStarkConfig>();
}

// ── Machine-layer composition types ─────────────────────────────────────────

#[test]
fn composition_types_are_send_sync() {
    use tabula_machine::{RootProof, SmtRootProof};

    assert_send_sync::<SmtRootProof>();
    // RootProof trait requires Send + Sync, so any `dyn RootProof` is too.
    assert_send::<Box<dyn RootProof>>();
    assert_sync::<Box<dyn RootProof>>();
}

// ── Machine-layer registry types ────────────────────────────────────────────

#[test]
fn registry_types_are_send_sync() {
    use tabula_machine::{ChipRegistry, RegisteredChip};

    // ChipRegistry holds `Vec<RegisteredChip>` with `Box<dyn AnyRap>`.
    // AnyRap : Send + Sync, so these must be thread-safe.
    assert_send_sync::<ChipRegistry>();
    assert_send_sync::<RegisteredChip>();
}

// ── Machine entry point ─────────────────────────────────────────────────────

#[test]
fn machine_is_send_sync() {
    use tabula_machine::TabulaMachine;

    assert_send_sync::<TabulaMachine>();
}

// ── Public statement ────────────────────────────────────────────────────────

#[test]
fn public_statement_is_send_sync() {
    use tabula_machine::PublicStatement;

    assert_send_sync::<PublicStatement>();
}

// ── Core identification types (re-exported via tabula-stark / tabula-chips) ─

#[test]
fn chip_identification_types_are_send_sync() {
    use tabula_stark::air::interaction::BusId;
    use tabula_stark::chips::{ChipId, ChipIdAllocator};

    assert_send_sync::<ChipId>();
    assert_send_sync::<BusId>();
    assert_send_sync::<ChipIdAllocator>();
}

// ── Core state types (from tabula-core) ─────────────────────────────────────

#[test]
fn core_state_types_are_send_sync() {
    use tabula_core::{
        CellKey, ColId, ColumnCommitmentId, ColumnDef, Digest, RowKey, StateRoot,
        TableCommitmentId, TableId, TableSchema, TxTypeId,
    };

    assert_send_sync::<TableId>();
    assert_send_sync::<ColId>();
    assert_send_sync::<RowKey>();
    assert_send_sync::<CellKey>();
    assert_send_sync::<TxTypeId>();
    assert_send_sync::<StateRoot>();
    assert_send_sync::<TableCommitmentId>();
    assert_send_sync::<ColumnCommitmentId>();
    assert_send_sync::<Digest>();
    assert_send_sync::<ColumnDef>();
    assert_send_sync::<TableSchema>();
}

// ── Core value types ────────────────────────────────────────────────────────

#[test]
fn core_value_types_are_send_sync() {
    use tabula_core::{Value, ValueType};

    assert_send_sync::<Value>();
    assert_send_sync::<ValueType>();
}

// ── Core transaction types ──────────────────────────────────────────────────

#[test]
fn core_transaction_types_are_send_sync() {
    use tabula_core::{Batch, ProgramBudgets, Transaction};

    assert_send_sync::<Transaction>();
    assert_send_sync::<Batch>();
    assert_send_sync::<ProgramBudgets>();
}

// ── Core execution output types ─────────────────────────────────────────────

#[test]
fn core_execution_types_are_send_sync() {
    use tabula_core::{
        AccessEvent, BatchResult, ETraceEventId, EmittedEvent, ExecutionConsistencyStatus,
        OpKind, TxResult,
    };

    assert_send_sync::<AccessEvent>();
    assert_send_sync::<BatchResult>();
    assert_send_sync::<TxResult>();
    assert_send_sync::<EmittedEvent>();
    assert_send_sync::<OpKind>();
    assert_send_sync::<ETraceEventId>();
    assert_send_sync::<ExecutionConsistencyStatus>();
}

// ── Core error types ────────────────────────────────────────────────────────

#[test]
fn core_error_types_are_send_sync() {
    use tabula_core::error::TabulaError;

    assert_send_sync::<TabulaError>();
}

// ── STARK interaction types ─────────────────────────────────────────────────

#[test]
fn stark_interaction_types_are_send_sync() {
    use p3_baby_bear::BabyBear;
    use tabula_stark::air::interaction::{
        AirInteraction, ColumnRef, Interaction, InteractionDirection, VirtualPairCol,
    };

    assert_send_sync::<InteractionDirection>();
    assert_send_sync::<ColumnRef>();
    assert_send_sync::<VirtualPairCol<BabyBear>>();
    assert_send_sync::<Interaction<BabyBear>>();
    assert_send_sync::<AirInteraction<BabyBear>>();
}

// ── STARK debug types ───────────────────────────────────────────────────────

#[test]
fn stark_debug_types_are_send_sync() {
    use p3_baby_bear::BabyBear;
    use tabula_stark::debug::{ChipRecord, ConstraintError, MultiChipError};

    assert_send_sync::<ConstraintError>();
    assert_send_sync::<MultiChipError>();
    assert_send_sync::<ChipRecord<BabyBear>>();
}

// ── STARK keygen types ──────────────────────────────────────────────────────

#[test]
fn stark_keygen_types_are_send_sync() {
    use tabula_stark::air::keygen::ChipKeygenInfo;

    assert_send_sync::<ChipKeygenInfo>();
}

// ── Extension field type ────────────────────────────────────────────────────

#[test]
fn extension_field_is_send_sync() {
    use tabula_machine::EF4;

    assert_send_sync::<EF4>();
}
