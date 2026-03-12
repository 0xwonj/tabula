//! Multi-proof STARK prover and verifier for Tabula batch proofs.
//!
//! Orchestrates a C+2 proof architecture (1 execution + C column + 1 root)
//! with shared Fiat-Shamir synchronization for LogUp challenges.
//!
//! ```ignore
//! let machine = TabulaMachine::new(&col_configs)?;
//! let traces = machine.build_traces(stores)?;
//! let proof = machine.prove(traces, &column_identities, statement)?;
//! machine.verify(&proof)?;
//! ```

mod any_rap;
mod blake3_pcs;
mod builder;
pub(crate) mod chip_ref;
pub mod column_scheme;
pub mod composition;
pub mod config;
pub mod extension;
pub mod keys;
mod machine;
pub mod prelude;
mod proof;
mod proof_instance;
pub mod property;
mod prove;
mod registry;
mod setup;
mod verify;

pub use any_rap::AnyRap;
pub use builder::MachineBuilder;
pub use column_scheme::{ColumnChipSet, ColumnScheme, SmtScheme, SsmcScheme};
pub use composition::{RootProof, SmtRootProof};
pub use config::{EF4, TabulaStarkConfig, default_config};
pub use extension::{ChipExtension, ExtensionContext};
pub use keys::{TabulaProvingKey, TabulaVerifyingKey, compute_external_buses};
pub use machine::TabulaMachine;
pub use proof::{
    ChipOpening, ColumnIdentity, ColumnProofEntry, ProofTier, ProveError, SubProofEnvelope,
    TabulaProof, VerificationError,
};
pub use property::{
    AggregateKind, PropertyError, PropertyOpening, PropertyQuery, PropertyQueryKind,
    PropertyWitness,
};
pub use registry::{ChipRegistry, RegisteredChip, SetupError};
pub use setup::{ColumnSetupConfig, ProofSetups, ProofTraces, TierSetup};
pub use tabula_stark::air::statement::PublicStatement;
